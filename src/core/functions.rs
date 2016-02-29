//! All the functions.
//!
//! ### Redundant Casts
//!
//! Redundant casts are temporary for development and will be removed.
//!
//! POSSIBLE TODO: Break this file up
//!

#![allow(dead_code)]

use std::ptr;
use std::mem;
use std::io::Read;
use std::ffi::CString;
use std::iter;
use libc::{size_t, c_void};
use num::{FromPrimitive};

use cl_h::{self, Status, cl_bool, cl_int, cl_uint, cl_platform_id, cl_device_id, cl_device_type, cl_device_info, cl_platform_info, cl_context, cl_context_info, cl_context_properties, cl_image_format, cl_image_desc, cl_kernel, cl_program_build_info, cl_mem, cl_mem_info, cl_mem_flags, cl_mem_object_type, cl_buffer_create_type, cl_event, cl_program, cl_addressing_mode, cl_filter_mode, cl_command_queue_info, cl_command_queue, cl_image_info, cl_sampler, cl_sampler_info, cl_program_info, cl_kernel_info, cl_kernel_arg_info, cl_kernel_work_group_info, cl_event_info, cl_profiling_info};

use error::{Error as OclError, Result as OclResult};
use core::{self, DEVICES_MAX, OclNum, PlatformId, DeviceId, Context, ContextProperties, ContextInfo, ContextInfoResult,  MemFlags, CommandQueue, Mem, MemObjectType, Program, Kernel, ClEventPtrNew, Event, EventList, Sampler, KernelArg, DeviceType, ImageFormat, ImageDescriptor, CommandExecutionStatus, AddressingMode, FilterMode, PlatformInfo, PlatformInfoResult, DeviceInfo, DeviceInfoResult, CommandQueueInfo, CommandQueueInfoResult, MemInfo, MemInfoResult, ImageInfo, ImageInfoResult, SamplerInfo, SamplerInfoResult, ProgramInfo, ProgramInfoResult, ProgramBuildInfo, ProgramBuildInfoResult, KernelInfo, KernelInfoResult, KernelArgInfo, KernelArgInfoResult, KernelWorkGroupInfo, KernelWorkGroupInfoResult, ClEventRef, EventInfo, EventInfoResult, ProfilingInfo, ProfilingInfoResult, CreateContextCallbackFn, UserDataPtr, ClPlatformIdPtr, ClDeviceIdPtr, EventCallbackFn, BuildProgramCallbackFn, MemMigrationFlags, MapFlags, BufferRegion, BufferCreateType};


//============================================================================
//============================================================================
//=========================== SUPPORT FUNCTIONS ==============================
//============================================================================
//============================================================================

/// Converts the `cl_int` errcode into a string containing the associated
/// constant name.
fn errcode_string(errcode: cl_int) -> String {
    match Status::from_i32(errcode) {
        Some(cls) => format!("{:?}", cls),
        None => format!("[Unknown Error Code: {}]", errcode as i64),
    }
}

/// Evaluates `errcode` and returns an `Err` with a failure message if it is
/// not 0.
///
/// [NAME?]: Is this an idiomatic name for this function?
///
/// TODO: Possibly convert this to a macro of some sort.
fn errcode_try(message: &str, errcode: cl_int) -> OclResult<()> {
    if errcode == cl_h::Status::CL_SUCCESS as cl_int {
        Ok(())
    } else {
        OclError::errcode(errcode, 
            format!(" {{ OPENCL ERROR: \"{} failed with code [{}]: {}\" }} ", 
                message, errcode, errcode_string(errcode))
        )
    }
}

/// Evaluates `errcode` and panics with a failure message if it is not 0.
fn errcode_assert(message: &str, errcode: cl_int) {
    errcode_try(message, errcode).unwrap();
}

/// Maps options of slices to pointers and a length.
fn resolve_event_ptrs<E: ClEventPtrNew>(wait_list: Option<&EventList>, new_event: Option<&mut E>,
        ) -> OclResult<(cl_uint, *const cl_event, *mut cl_event)> 
{
    // If the wait list is empty or if its containing option is none, map to (0, null),
    // otherwise map to the length and pointer (driver doesn't want an empty list):    
    let (wait_list_len, wait_list_ptr) = match wait_list {
        Some(wl) => {

            // [DEBUG]:
            // for event in wl.iter() {
            //     println!("EventInfo::ReferenceCount: {}", 
            //         get_event_info(event, EventInfo::ReferenceCount).unwrap());
            //     println!("EventInfo::CommandExecutionStatus: {}", 
            //         get_event_info(event, EventInfo::CommandExecutionStatus).unwrap());
            //     println!("EventInfo::CommandType: {}", 
            //         get_event_info(event, EventInfo::CommandType).unwrap());
            //     println!("Derefing...");
            //     println!("Ptr val: {:?}", *event);
            //     // println!("Size of 'E': {}", mem::size_of::<E>());
            //     println!("Size of 'Event': {}", mem::size_of::<Event>());
            //     println!("Size of 'usize': {}", mem::size_of::<usize>());
            //     println!("Size of '*const c_void': {}", mem::size_of::<*const c_void>());
            //     println!("Size of 'cl_event': {}", mem::size_of::<cl_event>());
            // }
            // [/DEBUG]

            if wl.count() > 0 {
                // (wl.len() as cl_uint, wl.as_ptr() as *const cl_event)
                (wl.count(), unsafe { wl.as_ptr_ptr() } )
            } else {
                (0, ptr::null_mut() as *const cl_event)
            }
        },
        None => (0, ptr::null_mut() as *const cl_event),
    };

    let new_event_ptr = match new_event {
        // Some(ne) => ne as *mut _ as *mut cl_event,
        Some(ne) => try!(ne.ptr_mut_ptr_new()),
        None => ptr::null_mut() as *mut cl_event,
    };

    // [DEBUG]:
    // println!("wait_list_len: {}, wait_list_ptr: {:?}, new_event_ptr:{:?}",
    //     wait_list_len, wait_list_ptr, new_event_ptr);

    

    Ok((wait_list_len, wait_list_ptr, new_event_ptr))
}

/// Converts an array option reference into a pointer to the contained array.
fn resolve_work_dims(work_dims: &Option<[usize; 3]>) -> *const size_t {
    match work_dims {
        &Some(ref w) => w as *const [usize; 3] as *const size_t,
        &None => 0 as *const size_t,
    }
}



/// If the program pointed to by `cl_program` for any of the devices listed in 
/// `device_ids` has a build log of any length, it will be returned as an 
/// errcode result.
///
pub fn program_build_err<D: ClDeviceIdPtr>(program: &Program, device_ids: &[D]) -> OclResult<()> {
    let mut size = 0 as size_t;

    for device_id in device_ids.iter() {
        unsafe {
            let name = cl_h::CL_PROGRAM_BUILD_LOG as cl_program_build_info;

            let mut errcode = cl_h::clGetProgramBuildInfo(
                program.as_ptr(),
                device_id.as_ptr(),
                name,
                0,
                ptr::null_mut(),
                &mut size,
            );
            errcode_assert("clGetProgramBuildInfo(size)", errcode);

            let mut pbi: Vec<u8> = iter::repeat(32u8).take(size as usize).collect();

            errcode = cl_h::clGetProgramBuildInfo(
                program.as_ptr(),
                device_id.as_ptr(),
                name,
                size,
                pbi.as_mut_ptr() as *mut c_void,
                ptr::null_mut(),
            );
            errcode_assert("clGetProgramBuildInfo()", errcode);

            if size > 1 {
                let pbi_nonull = try!(String::from_utf8(pbi));
                let pbi_errcode_string = format!(
                    "\n\n\
                    ###################### OPENCL PROGRAM BUILD DEBUG OUTPUT ######################\
                    \n\n{}\n\
                    ###############################################################################\
                    \n\n",
                    pbi_nonull);

                return OclError::err(pbi_errcode_string);
            }
        }
    }

    Ok(())
}


//============================================================================
//============================================================================
//======================= OPENCL FUNCTION WRAPPERS ===========================
//============================================================================
//============================================================================

//============================================================================
//============================= Platform API =================================
//============================================================================

/// Returns a list of available platforms as 'core' objects.
// TODO: Get rid of manual vec allocation now that PlatformId implements Clone.
pub fn get_platform_ids() -> OclResult<Vec<PlatformId>> {
    let mut num_platforms = 0 as cl_uint;
    
    // Get a count of available platforms:
    let mut errcode: cl_int = unsafe { 
        cl_h::clGetPlatformIDs(0, ptr::null_mut(), &mut num_platforms) 
    };
    try!(errcode_try("clGetPlatformIDs()", errcode));

    // Create a vec with the appropriate size:
    let mut null_vec: Vec<usize> = iter::repeat(0).take(num_platforms as usize).collect();
    let (ptr, len, cap) = (null_vec.as_mut_ptr(), null_vec.len(), null_vec.capacity());

    // Steal the vec's soul:
    let mut platforms: Vec<PlatformId> = unsafe {
        mem::forget(null_vec);
        Vec::from_raw_parts(ptr as *mut PlatformId, len, cap)
    };

    errcode = unsafe {
        cl_h::clGetPlatformIDs(
            num_platforms, 
            platforms.as_mut_ptr() as *mut cl_platform_id, 
            ptr::null_mut()
        )
    };
    try!(errcode_try("clGetPlatformIDs()", errcode));
    
    Ok(platforms)
}

/// Returns platform information of the requested type.
pub fn get_platform_info<P: ClPlatformIdPtr>(platform: Option<P>, request_param: PlatformInfo,
        ) -> OclResult<PlatformInfoResult> 
{
    let platform_ptr: cl_platform_id = match platform {
        Some(p) => unsafe { p.as_ptr() },
        None => ptr::null_mut() as cl_platform_id,
    };

    let mut size = 0 as size_t;

    unsafe {
        try!(errcode_try("clGetPlatformInfo()", cl_h::clGetPlatformInfo(
            platform_ptr,
            request_param as cl_platform_info,
            0 as size_t,
            ptr::null_mut(),
            &mut size as *mut size_t,
        )));
    }
        
    let mut requested_value: Vec<u8> = iter::repeat(32u8).take(size as usize).collect();

    unsafe {
        try!(errcode_try("clGetPlatformInfo()", cl_h::clGetPlatformInfo(
            platform_ptr,
            request_param as cl_platform_info,
            size as size_t,
            requested_value.as_mut_ptr() as *mut c_void,
            ptr::null_mut() as *mut size_t,
        )));
    }

    PlatformInfoResult::from_bytes(request_param, requested_value)
}

//============================================================================
//============================= Device APIs  =================================
//============================================================================

/// Returns a list of available devices for a particular platform.
pub fn get_device_ids<P: ClPlatformIdPtr>(
            platform: Option<P>, 
            device_types: Option<DeviceType>,
        ) -> OclResult<Vec<DeviceId>> 
{
    let platform_ptr = unsafe { match platform {
        Some(plat) => plat.as_ptr(),
        None => try!(get_first_platform()).as_ptr(),
    } };

    let device_types = device_types.unwrap_or(core::DEVICE_TYPE_ALL);
    let mut devices_available: cl_uint = 0;

    let mut device_ids: Vec<DeviceId> = iter::repeat(unsafe { DeviceId::null() } )
        .take(DEVICES_MAX as usize).collect();

    let errcode = unsafe { cl_h::clGetDeviceIDs(
        platform_ptr, 
        device_types.bits() as cl_device_type,
        DEVICES_MAX, 
        device_ids.as_mut_ptr() as *mut cl_device_id,
        &mut devices_available,
    ) };
    try!(errcode_try("clGetDeviceIDs()", errcode));

    // Trim vec len:
    unsafe { device_ids.set_len(devices_available as usize); }
    device_ids.shrink_to_fit();

    Ok(device_ids)
}

/// Returns information about a device.
///
/// ### Stability (or lack thereof)
///
/// Currently returning only one (temporary) variant.
///
#[allow(unused_variables)]
pub fn get_device_info<D: ClDeviceIdPtr>(device: &D, info_request: DeviceInfo,
        ) -> OclResult<(DeviceInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetDeviceInfo(
        device.as_ptr() as cl_device_id,
        info_request as cl_device_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetDeviceInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetDeviceInfo(
        device.as_ptr() as cl_device_id,
        info_request as cl_device_info,
        info_value_size  as size_t,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };
    errcode_try("clGetDeviceInfo", errcode)
        .and(Ok(DeviceInfoResult::TemporaryPlaceholderVariant(result)))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_sub_devices() -> OclResult<()> {
    // clCreateSubDevices(in_device: cl_device_id,
    //                    properties: *const cl_device_partition_property,
    //                    num_devices: cl_uint,
    //                    out_devices: *mut cl_device_id,
    //                    num_devices_ret: *mut cl_uint) -> cl_int;
    unimplemented!();
}

/// Increments the reference count of a device.
pub unsafe fn retain_device(device: &DeviceId) -> OclResult<()> {
    errcode_try("clRetainDevice", cl_h::clRetainDevice(device.as_ptr()))
}

/// Decrements the reference count of a device.
pub unsafe fn release_device(device: &DeviceId) -> OclResult<()> {
    errcode_try("clReleaseDevice", cl_h::clReleaseDevice(device.as_ptr())) 
}

//============================================================================
//============================= Context APIs  ================================
//============================================================================

/// Returns a new context pointer valid for all devices in `device_ids`.
///
/// [FIXME]: Incomplete implementation. Callback and userdata unimplemented.
/// [FIXME]: Properties disabled.
///
//
// [NOTE]: Leave commented print statements intact until more `ContextProperties 
// variants are implemented.
pub fn create_context<D: ClDeviceIdPtr>(properties: &Option<ContextProperties>, device_ids: &Vec<D>,
            pfn_notify: Option<CreateContextCallbackFn>, user_data: Option<UserDataPtr>
        ) -> OclResult<Context> 
{
    if device_ids.len() == 0 {
        return OclError::err("ocl::core::create_context: No devices specified.");
    }

    // [DEBUG]: 
    // println!("CREATE_CONTEXT: ORIGINAL: properties: {:?}", properties);

    let properties_bytes: Vec<u8> = match properties {
        &Some(ref props) => props.to_bytes(),
        &None => Vec::<u8>::with_capacity(0),
    };

    // [DEBUG]: 
    // print!("CREATE_CONTEXT: BYTES: ");
    // util::print_bytes_as_hex(&properties_bytes);
    // print!("\n");

    // [FIXME]: Properties disabled:
    let properties_ptr = if properties_bytes.len() == 0 { 
        ptr::null() as *const cl_context_properties
    } else {
        // [FIXME]: Properties disabled.
        // properties_bytes.as_ptr()
        ptr::null() as *const cl_context_properties
    };

    // // [FIXME]: Disabled:
    // let pfn_notify_ptr = unsafe { match pfn_notify {
    //     // Some(cb) => mem::transmute(cb),
    //     Some(_) => mem::transmute(ptr::null::<fn()>()), 
    //     // Some(_) => ptr::null::<CreateContextCallbackFn>(),
    //     None => mem::transmute(ptr::null::<fn()>()), 
    //     // None => ptr::null::<CreateContextCallbackFn>(),
    // } };

    // [FIXME]: Disabled:
    let user_data_ptr = match user_data {
        // Some(ud_ptr) => ud_ptr,
        Some(_) => ptr::null_mut(),
        None => ptr::null_mut(), 
    };
    
    let mut errcode: cl_int = 0;

    // [FIXME]: Callback function and data unimplemented.
    let context = unsafe { Context::from_fresh_ptr(cl_h::clCreateContext(
        properties_ptr, 
        device_ids.len() as cl_uint, 
        device_ids.as_ptr()  as *const cl_device_id,
        pfn_notify,
        user_data_ptr,
        &mut errcode,
    )) };
    // [DEBUG]: 
    // println!("CREATE_CONTEXT: CONTEXT PTR: {:?}", context);
    errcode_try("clCreateContext()", errcode).and(Ok(context))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_context_from_type() -> OclResult<()> {
    // cl_h::clCreateContextFromType(properties: *mut cl_context_properties,
    //                            device_type: cl_device_type,
    //                            pfn_notify: extern fn (*mut c_char, *mut c_void, size_t, *mut c_void),
    //                            user_data: *mut c_void,
    //                            errcode_ret: *mut cl_int) -> cl_context;
    unimplemented!();
}

/// Increments the reference count of a context.
pub unsafe fn retain_context(context: &Context) -> OclResult<()> {
    errcode_try("clRetainContext", cl_h::clRetainContext(context.as_ptr()))
}

/// Decrements reference count of a context.
pub unsafe fn release_context(context: &Context) -> OclResult<()> {
    errcode_try("clReleaseContext", cl_h::clReleaseContext(context.as_ptr()))
}

/// Returns various kinds of context information.
///
/// [SDK Reference](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clGetContextInfo.html)
///
/// # Errors
///
/// Returns an error result for all the reasons listed in the SDK in addition 
/// to an additional error when called with `CL_CONTEXT_DEVICES` as described
/// in in the `verify_context()` documentation below.
pub fn get_context_info(context: &Context, request_param: ContextInfo,
        ) -> OclResult<(ContextInfoResult)>
{
   let mut result_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetContextInfo(   
        context.as_ptr() as cl_context,
        request_param as cl_context_info,
        0 as size_t,
        0 as *mut c_void,
        &mut result_size as *mut usize,
    ) };
    try!(errcode_try("clGetContextInfo", errcode));

    // Check for invalid context pointer (a potentially hard to track down bug)
    // using ridiculous and probably platform-specific logic [if the `Devices` 
    // variant is passed and we're not in the release config]:
    if !cfg!(release) {
        let err_if_zero_result_size = request_param as cl_context_info == cl_h::CL_CONTEXT_DEVICES;

        if result_size > 10000 || (result_size == 0 && err_if_zero_result_size) {
            return OclError::err("\n\nocl::core::context_info(): Possible invalid context detected. \n\
                Context info result size is either '> 10k bytes' or '== 0'. Almost certainly an \n\
                invalid context object. If not, please file an issue at: \n\
                https://github.com/cogciprocate/ocl/issues.\n\n");
        }
    }

    let mut result: Vec<u8> = iter::repeat(0).take(result_size).collect();

    let errcode = unsafe { cl_h::clGetContextInfo(   
        context.as_ptr() as cl_context,
        request_param as cl_context_info,
        result_size as size_t,
        result.as_mut_ptr() as *mut c_void,
        0 as *mut usize,
    ) };
    errcode_try("clGetContextInfo", errcode).and(
        ContextInfoResult::new(request_param, result))
}

//============================================================================
//========================== Command Queue APIs ==============================
//============================================================================

/// Returns a new command queue pointer.
pub fn create_command_queue<D: ClDeviceIdPtr>(
            context: &Context, 
            device: &D,
        ) -> OclResult<CommandQueue> 
{
    // Verify that the context is valid:
    try!(verify_context(context));

    let mut errcode: cl_int = 0;

    let cq = unsafe { CommandQueue::from_fresh_ptr(cl_h::clCreateCommandQueue(
        context.as_ptr(), 
        device.as_ptr(),
        cl_h::CL_QUEUE_PROFILING_ENABLE, 
        &mut errcode
    )) };
    errcode_try("clCreateCommandQueue", errcode).and(Ok(cq))
}

/// Increments the reference count of a command queue.
pub unsafe fn retain_command_queue(queue: &CommandQueue) -> OclResult<()> {
    errcode_try("clRetainCommandQueue", cl_h::clRetainCommandQueue(queue.as_ptr()))
}

/// Decrements the reference count of a command queue.
///
/// [FIXME]: Return result
pub unsafe fn release_command_queue(queue: &CommandQueue) -> OclResult<()> {
    errcode_try("clReleaseCommandQueue", 
        cl_h::clReleaseCommandQueue(queue.as_ptr()))
}

/// Returns information about a command queue
pub fn get_command_queue_info(queue: &CommandQueue, info_request: CommandQueueInfo,
        ) -> OclResult<(CommandQueueInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetCommandQueueInfo(
        queue.as_ptr() as cl_command_queue,
        info_request as cl_command_queue_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetCommandQueueInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetCommandQueueInfo(
        queue.as_ptr() as cl_command_queue,
        info_request as cl_command_queue_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    errcode_try("clGetCommandQueueInfo", errcode)
        .and(Ok(CommandQueueInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Memory Object APIs ==============================
//============================================================================

/// Returns a new buffer pointer with size (bytes): `len` * sizeof(T).
pub fn create_buffer<T: OclNum>(
            context: &Context,
            flags: MemFlags,
            len: usize,
            data: Option<&[T]>,
        ) -> OclResult<Mem> 
{
    // Verify that the context is valid:
    try!(verify_context(context));

    let mut errcode: cl_int = 0;

    let host_ptr = match data {
        Some(d) => {
            if d.len() != len { 
                return OclError::err("ocl::create_buffer: Data length mismatch.");
            }
            d.as_ptr() as cl_mem
        },
        None => ptr::null_mut(),
    };

    let buf_ptr = unsafe { cl_h::clCreateBuffer(
        context.as_ptr(), 
        flags.bits() as cl_mem_flags,
        len * mem::size_of::<T>(),
        host_ptr, 
        &mut errcode,
    ) };
    try!(errcode_try("create_buffer", errcode));
    debug_assert!(!buf_ptr.is_null());

    unsafe { Ok(Mem::from_fresh_ptr(buf_ptr)) }
}

/// [UNTESTED]
/// Creates a new buffer object (referred to as a sub-buffer object) from an
/// existing buffer object.
///
/// The returned sub-buffer has a number of caveats which can cause undefined
/// behavior.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clCreateSubBuffer.html)
///
pub fn create_sub_buffer(
            buffer: &Mem,
            flags: MemFlags,
            buffer_create_info: &BufferRegion,
        ) -> OclResult<Mem> 
{
    let buffer_create_type = BufferCreateType::Region;

    let mut errcode = 0i32;

    let sub_buf_ptr = unsafe { cl_h::clCreateSubBuffer(
        buffer.as_ptr(),
        flags.bits(),
        buffer_create_type as cl_buffer_create_type,
        buffer_create_info as *const _ as *const c_void,
        &mut errcode,
    ) };
    try!(errcode_try("clCreateSubBuffer", errcode));
    debug_assert!(!sub_buf_ptr.is_null()); 

    unsafe { Ok(Mem::from_fresh_ptr(sub_buf_ptr)) }    
}

/// Returns a new image (mem) pointer.
// [WORK IN PROGRESS]
pub fn create_image<T>(
            context: &Context,
            flags: MemFlags,
            format: &ImageFormat,
            desc: &ImageDescriptor,
            data: Option<&[T]>,
        ) -> OclResult<Mem> 
{
    // Verify that the context is valid:
    try!(verify_context(context));

    let mut errcode: cl_int = 0;
    
    let host_ptr = match data {
        Some(d) => {
            // [FIXME]: CALCULATE CORRECT IMAGE SIZE AND COMPARE
            // assert!(d.len() == len, "ocl::create_image(): Data length mismatch.");
            d.as_ptr() as cl_mem
        },
        None => ptr::null_mut(),
    };

    let image_ptr = unsafe { cl_h::clCreateImage(
        context.as_ptr(),
        flags.bits() as cl_mem_flags,
        &format.to_raw() as *const cl_image_format,
        &desc.to_raw() as *const cl_image_desc,
        host_ptr,
        &mut errcode as *mut cl_int,
    ) }; 
    errcode_assert("clCreateImage", errcode);
    debug_assert!(!image_ptr.is_null());

    unsafe { Ok(Mem::from_fresh_ptr(image_ptr)) }
}

/// Increments the reference counter of a mem object.
pub unsafe fn retain_mem_object(mem: &Mem) -> OclResult<()> {
    errcode_try("clRetainMemObject", cl_h::clRetainMemObject(mem.as_ptr()))
}

/// Decrements the reference counter of a mem object.
pub unsafe fn release_mem_object(mem: &Mem) -> OclResult<()> {
    errcode_try("clReleaseMemObject", cl_h::clReleaseMemObject(mem.as_ptr()))
}

/// Returns a list of supported image formats.
///
/// # Example
/// 
/// ```notest
/// let context = Context::builder().build().unwrap();
/// 
/// let img_fmts = core::get_supported_image_formats(context, 
///    core::MEM_READ_WRITE, core::MemObjectType::Image2d)
/// ```
pub fn get_supported_image_formats(
            context: &Context, 
            flags: MemFlags,
            image_type: MemObjectType,
        ) -> OclResult<Vec<ImageFormat>> 
{
    let mut num_image_formats = 0 as cl_uint;

    let errcode = unsafe { cl_h::clGetSupportedImageFormats(
        context.as_ptr(),
        flags.bits() as cl_mem_flags,
        image_type as cl_mem_object_type,
        0 as cl_uint,
        ptr::null_mut() as *mut cl_image_format,
        &mut num_image_formats as *mut cl_uint,
    ) };
    try!(errcode_try("clGetSupportedImageFormats", errcode));

    let mut image_formats: Vec<cl_image_format> = (0..(num_image_formats as usize)).map(|_| {
           ImageFormat::new_raw()
        } ).collect();

    debug_assert!(image_formats.len() == num_image_formats as usize && image_formats.len() > 0);

    let errcode = unsafe { cl_h::clGetSupportedImageFormats(
        context.as_ptr(),
        flags.bits() as cl_mem_flags,
        image_type as cl_mem_object_type,
        num_image_formats,
        image_formats.as_mut_ptr() as *mut _ as *mut cl_image_format,
        0 as *mut cl_uint,
    ) };
    try!(errcode_try("clGetSupportedImageFormats", errcode));

    ImageFormat::list_from_raw(image_formats)
}


/// Get mem object info.
pub fn get_mem_object_info(obj: &Mem, info_request: MemInfo,
        ) -> OclResult<(MemInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetMemObjectInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_mem_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetMemObjectInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetMemObjectInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_mem_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    errcode_try("clGetMemObjectInfo", errcode)
        .and(Ok(MemInfoResult::TemporaryPlaceholderVariant(result)))
}


/// Get image info.
pub fn get_image_info(obj: &Mem, info_request: ImageInfo) -> OclResult<(ImageInfoResult)> {
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetImageInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_image_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetImageInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetImageInfo(
        obj.as_ptr() as cl_mem,
        info_request as cl_image_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetImageInfo", errcode)
        .and(Ok(ImageInfoResult::TemporaryPlaceholderVariant(result)))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn set_mem_object_destructor_callback() -> OclResult<()> {
    // cl_h::clSetMemObjectDestructorCallback(memobj: cl_mem,
    //                                     pfn_notify: extern fn (cl_mem, *mut c_void),
    //                                     user_data: *mut c_void) -> cl_int;
    unimplemented!();
}

//============================================================================
//============================= Sampler APIs =================================
//============================================================================

/// [UNTESTED]
/// Creates and returns a new sampler object.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clCreateSampler.html)
pub fn create_sampler(context: &Context, normalize_coords: bool, addressing_mode: AddressingMode,
            filter_mode: FilterMode) -> OclResult<Sampler> 
{
    let mut errcode = 0;

    let sampler = unsafe { Sampler::from_fresh_ptr(cl_h::clCreateSampler(
        context.as_ptr(),
        normalize_coords as cl_bool,
        addressing_mode as cl_addressing_mode,
        filter_mode as cl_filter_mode,
        &mut errcode,
    )) };

    errcode_try("clCreateSampler", errcode).and(Ok(sampler))
}

/// Increments a sampler reference counter.
pub unsafe fn retain_sampler(sampler: &Sampler) -> OclResult<()> {
    errcode_try("clRetainSampler", cl_h::clRetainSampler(sampler.as_ptr()))
}

/// Decrements a sampler reference counter.
pub unsafe fn release_sampler(sampler: &Sampler) -> OclResult<()> {
    errcode_try("clReleaseSampler", cl_h::clReleaseSampler(sampler.as_ptr()))
}

/// Returns information about the sampler object.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clGetSamplerInfo.html)
pub fn get_sampler_info(obj: &Sampler, info_request: SamplerInfo,
    ) -> OclResult<(SamplerInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetSamplerInfo(
        obj.as_ptr() as cl_sampler,
        info_request as cl_sampler_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetSamplerInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetSamplerInfo(
        obj.as_ptr() as cl_sampler,
        info_request as cl_sampler_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    errcode_try("clGetSamplerInfo", errcode)
        .and(Ok(SamplerInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Program Object APIs =============================
//============================================================================

/// Creates a new program.
pub fn create_program_with_source(
            context: &Context, 
            src_strings: &[CString],
        ) -> OclResult<Program> 
{
    // Verify that the context is valid:
    try!(verify_context(context));

    // Lengths (not including \0 terminator) of each string:
    let ks_lens: Vec<usize> = src_strings.iter().map(|cs| cs.as_bytes().len()).collect();  

    // Pointers to each string:
    let kern_string_ptrs: Vec<*const i8> = src_strings.iter().map(|cs| cs.as_ptr()).collect();

    let mut errcode: cl_int = 0;
    
    let program = unsafe { cl_h::clCreateProgramWithSource(
        context.as_ptr(), 
        kern_string_ptrs.len() as cl_uint,
        kern_string_ptrs.as_ptr() as *const *const i8,
        ks_lens.as_ptr() as *const usize,
        &mut errcode,
    ) };
    try!(errcode_try("clCreateProgramWithSource", errcode));

    unsafe { Ok(Program::from_fresh_ptr(program)) }
}

/// [UNTESTED]
/// Creates a program object for a context, and loads the binary bits
/// specified by binary into the program object.
///
/// [SDK Docs]: https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clCreateProgramWithBinary.html
///
pub fn create_program_with_binary<D: ClDeviceIdPtr>(
            context: &Context, 
            devices: &[D],            
            binaries: &[&[u8]],
        ) -> OclResult<(Program)> 
{
    // assert!(devices.len() > 0);
    // assert!(devices.len() == binaries.len());
    if devices.len() == 0 { return OclError::err("ocl::create_program_with_binary: \
        Length of 'devices' must be greater than zero."); }
    if devices.len() != binaries.len() { return OclError::err("ocl::create_program_with_binary: \
        Length of 'devices' must equal the length of 'binaries' (e.g. one binary per device)."); }

    let lengths: Vec<usize> = binaries.iter().map(|bin| bin.len()).collect();
    let mut binary_status: Vec<i32> = iter::repeat(0).take(devices.len()).collect();
    let mut errcode: cl_int = 0;
    
    let program = unsafe { cl_h::clCreateProgramWithBinary(
        context.as_ptr(),         
        devices.len() as u32,
        devices.as_ptr() as *const _ as *const cl_device_id,
        lengths.as_ptr(),
        binaries.as_ptr() as *const *const u8,
        binary_status.as_mut_ptr(),
        &mut errcode,
    ) };
    try!(errcode_try("clCreateProgramWithBinary", errcode));

    for i in 0..binary_status.len() {
        try!(errcode_try(&format!("clCreateProgramWithBinary: Device [{}]", i), binary_status[i]));
    }

    unsafe { Ok(Program::from_fresh_ptr(program)) }
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_program_with_built_in_kernels() -> OclResult<()> {
    // clCreateProgramWithBuiltInKernels(context: cl_context,
    //                                  num_devices: cl_uint,
    //                                  device_list: *const cl_device_id,
    //                                  kernel_names: *mut char,
    //                                  errcode_ret: *mut cl_int) -> cl_program;
    unimplemented!();
}

/// Increments a program reference counter.
pub unsafe fn retain_program(program: &Program) -> OclResult<()> {
    errcode_try("clRetainProgram", cl_h::clRetainProgram(program.as_ptr()))
}

/// Decrements a program reference counter.
pub unsafe fn release_program(program: &Program) -> OclResult<()> {
    errcode_try("clReleaseKernel", cl_h::clReleaseProgram(program.as_ptr()))
}

pub struct UserDataPh(usize);

impl UserDataPh {
    fn unwrapped(&self) -> *mut c_void {
        ptr::null_mut()
    }
}

/// Builds a program.
///
/// Callback functions are not yet supported.
pub fn build_program<D: ClDeviceIdPtr>(
            program: &Program,
            devices: &[D],
            options: &CString,
            pfn_notify: Option<BuildProgramCallbackFn>,
            user_data: Option<Box<UserDataPh>>,
        ) -> OclResult<()> 
{
    assert!(pfn_notify.is_none() && user_data.is_none(),
        "ocl::core::build_program(): Callback functions not yet implemented.");

    let user_data = match user_data {
        Some(ud) => ud.unwrapped(),
        None => ptr::null_mut(),
    };

    let errcode = unsafe { cl_h::clBuildProgram(
        program.as_ptr() as cl_program,
        devices.len() as cl_uint,
        devices.as_ptr() as *const cl_device_id, 
        options.as_ptr() as *const i8,
        pfn_notify,
        user_data,
    ) };  

    if errcode < 0 {
        program_build_err(program, devices)
    } else {
        try!(errcode_try("clBuildProgram()", errcode));
        Ok(()) 
    }
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn compile_program() -> OclResult<()> {
    // clCompileProgram(program: cl_program,
    //                 num_devices: cl_uint,
    //                 device_list: *const cl_device_id,
    //                 options: *const c_char, 
    //                 num_input_headers: cl_uint,
    //                 input_headers: *const cl_program,
    //                 header_include_names: *const *const c_char,
    //                 pfn_notify: extern fn (program: cl_program, user_data: *mut c_void),
    //                 user_data: *mut c_void) -> cl_int;
    unimplemented!();
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn link_program() -> OclResult<()> {
    // clLinkProgram(context: cl_context,
    //               num_devices: cl_uint,
    //               device_list: *const cl_device_id,
    //               options: *const c_char, 
    //               num_input_programs: cl_uint,
    //               input_programs: *const cl_program,
    //               pfn_notify: extern fn (program: cl_program, user_data: *mut c_void),
    //               user_data: *mut c_void,
    //               errcode_ret: *mut cl_int) -> cl_program;
    unimplemented!();
}

/// [UNTESTED]
/// Unloads a platform compiler.
pub fn unload_platform_compiler(platform: &PlatformId) -> OclResult<()> {
    unsafe { errcode_try("clUnloadPlatformCompiler", 
        cl_h::clUnloadPlatformCompiler(platform.as_ptr())) }
}

/// Get program info.
pub fn get_program_info(obj: &Program, info_request: ProgramInfo,
        ) -> OclResult<(ProgramInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetProgramInfo(
        obj.as_ptr() as cl_program,
        info_request as cl_program_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetProgramInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetProgramInfo(
        obj.as_ptr() as cl_program,
        info_request as cl_program_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    errcode_try("clGetProgramInfo", errcode)
        .and(Ok(ProgramInfoResult::TemporaryPlaceholderVariant(result)))
}

/// Get program build info.
pub fn get_program_build_info<D: ClDeviceIdPtr>(obj: &Program, device_obj: &D, info_request: ProgramBuildInfo,
        ) -> OclResult<(ProgramBuildInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetProgramBuildInfo(
        obj.as_ptr() as cl_program,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_program_build_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetProgramBuildInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetProgramBuildInfo(
        obj.as_ptr() as cl_program,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_program_build_info,
        info_value_size as size_t,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    errcode_try("clGetProgramBuildInfo", errcode)
        .and(Ok(ProgramBuildInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Kernel Object APIs ==============================
//============================================================================

/// Returns a new kernel pointer.
pub fn create_kernel(
            program: &Program, 
            name: &str,
        ) -> OclResult<Kernel> 
{
    let mut err: cl_int = 0;

    let kernel = unsafe { Kernel::from_fresh_ptr(cl_h::clCreateKernel(
        program.as_ptr(),
        try!(CString::new(name.as_bytes())).as_ptr(), 
        &mut err,
    )) };
    errcode_try(&format!("clCreateKernel('{}'):", &name), err).and(Ok(kernel))
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn create_kernels_in_program() -> OclResult<()> {
    // cl_h::clCreateKernelsInProgram(program: cl_program,
    //                             num_kernels: cl_uint,
    //                             kernels: *mut cl_kernel,
    //                             num_kernels_ret: *mut cl_uint) -> cl_int;
    unimplemented!();
}

/// Increments a kernel reference counter.
pub unsafe fn retain_kernel(kernel: &Kernel) -> OclResult<()> {
    errcode_try("clRetainKernel", cl_h::clRetainKernel(kernel.as_ptr()))
}

/// Decrements a kernel reference counter.
pub unsafe fn release_kernel(kernel: &Kernel) -> OclResult<()> {
    errcode_try("clReleaseKernel", cl_h::clReleaseKernel(kernel.as_ptr()))
}


/// Sets the argument value for a specific argument of a kernel.
///
/// [SDK Documentation](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clSetKernelArg.html)
///
/// [FIXME: Remove] `kernel_name` is for error reporting and is optional but highly recommended.
///
/// TODO: Remove `name` parameter and lookup name with `get_kernel_info` instead.
pub fn set_kernel_arg<T: OclNum>(kernel: &Kernel, arg_index: u32, arg: KernelArg<T>,
            name: Option<&str>) -> OclResult<()>
{
    // [DEBUG] LEAVE THIS HERE:
    // println!("SET_KERNEL_ARG: KERNELARG: {:?}", arg);

    let (arg_size, arg_value): (size_t, *const c_void) = match arg {
        KernelArg::Mem(mem_core_ref) => (
            mem::size_of::<cl_mem>() as size_t, 
            // (mem_obj.as_ptr() as *mut c_void) as *const c_void
            mem_core_ref as *const _ as *const c_void
        ),
        KernelArg::Sampler(smplr_core_ref) => (
            mem::size_of::<cl_sampler>() as size_t, 
            // (smplr.as_ptr() as *mut c_void) as *const c_void)
            smplr_core_ref as *const _ as *const c_void
        ),
        KernelArg::Scalar(scalar_ref) => (
            mem::size_of::<T>() as size_t, 
            // scalar as *const _ as *const c_void
            scalar_ref as *const T as *const c_void
        ),
        KernelArg::Vector(slice)=> (
            (mem::size_of::<T>() * slice.len()) as size_t,
            // vector as *const _ as *const c_void
            slice as *const _ as *const c_void
        ),
        KernelArg::Local(length) => (
            (mem::size_of::<T>() * length) as size_t,
            ptr::null()
        ),
        KernelArg::UnsafePointer { size, value } => (size, value),
        _ => (mem::size_of::<*const c_void>() as size_t, ptr::null()),
    };

    // [DEBUG] LEAVE THIS HERE:
    // println!("SET_KERNEL_ARG: KERNEL: {:?}", kernel);
    // println!("SET_KERNEL_ARG: index: {:?}", arg_index);
    // println!("SET_KERNEL_ARG: size: {:?}", arg_size);
    // println!("SET_KERNEL_ARG: value: {:?}", arg_value);
    // println!("SET_KERNEL_ARG: name: {:?}", name);
    // print!("\n");
    // [/DEBUG]

    let err = unsafe { cl_h::clSetKernelArg(
            kernel.as_ptr(), 
            arg_index,
            arg_size, 
            arg_value,
    ) };

    let err_pre = format!("clSetKernelArg('{}'):", name.unwrap_or(""));

    errcode_try(&err_pre, err)
} 

/// Get kernel info.
pub fn get_kernel_info(obj: &Kernel, info_request: KernelInfo,
        ) -> OclResult<(KernelInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetKernelInfo(
        obj.as_ptr() as cl_kernel,
        info_request as cl_kernel_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetKernelInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetKernelInfo(
        obj.as_ptr() as cl_kernel,
        info_request as cl_kernel_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetKernelInfo", errcode)
        .and(Ok(KernelInfoResult::TemporaryPlaceholderVariant(result)))
}

/// Get kernel arg info.
pub fn get_kernel_arg_info(obj: &Kernel, arg_index: u32, info_request: KernelArgInfo,
        ) -> OclResult<(KernelArgInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetKernelArgInfo(
        obj.as_ptr() as cl_kernel,
        arg_index as cl_uint,
        info_request as cl_kernel_arg_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetKernelArgInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetKernelArgInfo(
        obj.as_ptr() as cl_kernel,
        arg_index as cl_uint,
        info_request as cl_kernel_arg_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetKernelArgInfo", errcode)
        .and(Ok(KernelArgInfoResult::TemporaryPlaceholderVariant(result)))
}

/// Get kernel work group info.
pub fn get_kernel_work_group_info<D: ClDeviceIdPtr>(obj: &Kernel, device_obj: &D, 
            info_request: KernelWorkGroupInfo) -> OclResult<(KernelWorkGroupInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetKernelWorkGroupInfo(
        obj.as_ptr() as cl_kernel,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_kernel_work_group_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetKernelWorkGroupInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetKernelWorkGroupInfo(
        obj.as_ptr() as cl_kernel,
        device_obj.as_ptr() as cl_device_id,
        info_request as cl_kernel_work_group_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetKernelWorkGroupInfo", errcode)
        .and(Ok(KernelWorkGroupInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================== Event Object APIs ===============================
//============================================================================

/// Blocks until the first `num_events` events in `event_list` are complete.
pub fn wait_for_events(num_events: u32, event_list: &EventList) {
    assert!(event_list.count() >= num_events);

    let errcode = unsafe {
        cl_h::clWaitForEvents(num_events, event_list.as_ptr_ptr())
    };

    errcode_assert("clWaitForEvents", errcode);
}

/// Get event info.
pub fn get_event_info(event: &Event, info_request: EventInfo,
        ) -> OclResult<(EventInfoResult)> 
{
    let mut info_value_size: size_t = 0;

    let errcode = unsafe { cl_h::clGetEventInfo(
        *event.as_ptr_ref(),
        info_request as cl_event_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetEventInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetEventInfo(
        *event.as_ptr_ref(),
        info_request as cl_event_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    errcode_try("clGetEventInfo", errcode)
        .and(Ok(EventInfoResult::TemporaryPlaceholderVariant(result)))
}

/// [UNTESTED]
/// Creates an event not already associated with any command.
pub fn create_user_event(context: &Context) -> OclResult<(Event)> {
    let mut errcode = 0;
    let event = unsafe { Event::from_fresh_ptr(cl_h::clCreateUserEvent(context.as_ptr(), &mut errcode)) };
    errcode_try("clCreateUserEvent", errcode).and(Ok(event))
}

/// Increments an event's reference counter.
pub unsafe fn retain_event<'e, E: ClEventRef<'e>>(event: &'e E) -> OclResult<()> {
    // cl_h::clRetainEvent(event: cl_event) -> cl_int;
    errcode_try("clRetainEvent", cl_h::clRetainEvent(*event.as_ptr_ref()))
}

/// Decrements an event's reference counter.
pub unsafe fn release_event<'e, E: ClEventRef<'e>>(event: &'e E) -> OclResult<()> {
    errcode_try("clReleaseEvent", cl_h::clReleaseEvent(*event.as_ptr_ref()))
}

/// [UNTESTED]
/// Updates a user events status.
pub fn set_user_event_status<'e, E: ClEventRef<'e>>(event: &'e E, 
            execution_status: CommandExecutionStatus) -> OclResult<()>
{
    unsafe { errcode_try("clSetUserEventStatus", cl_h::clSetUserEventStatus(
        *event.as_ptr_ref(), execution_status as cl_int)) }
}

/// Sets a callback function which is called as soon as the `callback_trigger`
/// status is reached.
pub unsafe fn set_event_callback<'e, E: ClEventRef<'e>>(
            event: &'e E,
            callback_trigger: CommandExecutionStatus,
            callback_receiver: Option<EventCallbackFn>,
            user_data: *mut c_void,
        ) -> OclResult<()> 
{
    errcode_try("clSetEventCallback", cl_h::clSetEventCallback(
        *event.as_ptr_ref(), 
        callback_trigger as cl_int, 
        callback_receiver, 
        user_data,
    ))
}

//============================================================================
//============================ Profiling APIs ================================
//============================================================================

/// Get event profiling info (for debugging / benchmarking).
pub fn get_event_profiling_info(event: &Event, info_request: ProfilingInfo,
        ) -> OclResult<(ProfilingInfoResult)> 
{
    let mut info_value_size: size_t = 0;
    let event: cl_event = unsafe { *event.as_ptr_ref() };

    let errcode = unsafe { cl_h::clGetEventProfilingInfo(
        event,
        info_request as cl_profiling_info,
        0 as size_t,
        0 as *mut c_void,
        &mut info_value_size as *mut size_t,
    ) };
    try!(errcode_try("clGetEventProfilingInfo", errcode));

    let mut result: Vec<u8> = iter::repeat(0u8).take(info_value_size).collect();

    let errcode = unsafe { cl_h::clGetEventProfilingInfo(
        event,
        info_request as cl_profiling_info,
        info_value_size,
        result.as_mut_ptr() as *mut _ as *mut c_void,
        0 as *mut size_t,
    ) };    
    // println!("GET_COMMAND_QUEUE_INFO(): errcode: {}, result: {:?}", errcode, result);
    errcode_try("clGetEventProfilingInfo", errcode)
        .and(Ok(ProfilingInfoResult::TemporaryPlaceholderVariant(result)))
}

//============================================================================
//========================= Flush and Finish APIs ============================
//============================================================================

/// [UNTESTED]
/// Flushes a command queue.
///
/// Issues all previously queued OpenCL commands in a command-queue to the 
/// device associated with the command-queue.
pub fn flush(command_queue: &CommandQueue) -> OclResult<()> {
    unsafe { errcode_try("clFlush", cl_h::clFlush(command_queue.as_ptr())) }
}

/// Waits for a queue to finish.
///
/// Blocks until all previously queued OpenCL commands in a command-queue are 
/// issued to the associated device and have completed.
pub fn finish(command_queue: &CommandQueue) -> OclResult<()> {
    unsafe { 
        let errcode = cl_h::clFinish(command_queue.as_ptr());
        errcode_try("clFinish()", errcode)
    }
}

//============================================================================
//======================= Enqueued Commands APIs =============================
//============================================================================

/// Enqueues a read from device memory referred to by `buffer` to device memory,
/// `data`.
///
/// ### Safety
///
/// Caller must ensure that `data` lives until the read is complete. Use
/// `new_event` to monitor it (use [`core::EventList::last_clone`] if passing
/// an event list as `new_event`).
///
///
/// [`core::EventList::get_clone`]: http://doc.cogciprocate.com/ocl/core/struct.EventList.html#method.last_clone
///
pub unsafe fn enqueue_read_buffer<T: OclNum, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            buffer: &Mem, 
            block: bool,
            offset: usize,
            data: &mut [T],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = cl_h::clEnqueueReadBuffer(
        command_queue.as_ptr(), 
        buffer.as_ptr(), 
        block as cl_uint, 
        offset, 
        (data.len() * mem::size_of::<T>()) as size_t, 
        data.as_ptr() as cl_mem, 
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    );

    errcode_try("clEnqueueReadBuffer()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to read from a rectangular region from a buffer object to host memory.
///
/// ### Safety
///
/// Caller must ensure that `data` lives until the read is complete. Use
/// `new_event` to monitor it (use [`core::EventList::last_clone`] if passing
/// an event list as `new_event`).
///
/// ### Official Documentation
///
/// [SDK - clEnqueueReadBufferRect](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueReadBufferRect.html)
///
///
/// [`core::EventList::get_clone`]: http://doc.cogciprocate.com/ocl/core/struct.EventList.html#method.last_clone
///
pub unsafe fn enqueue_read_buffer_rect<T: OclNum, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            buffer: &Mem, 
            block: bool,
            buffer_origin: [usize; 3],
            host_origin: [usize; 3],
            region: [usize; 3],
            buffer_row_pitch: usize,
            buffer_slc_pitch: usize,
            host_row_pitch: usize,
            host_slc_pitch: usize,
            data: &mut [T],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = cl_h::clEnqueueReadBufferRect(
        command_queue.as_ptr(), 
        buffer.as_ptr(), 
        block as cl_uint,
        &buffer_origin as *const _ as *const usize,
        &host_origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        buffer_row_pitch,
        buffer_slc_pitch,
        host_row_pitch,
        host_slc_pitch,
        data.as_ptr() as cl_mem, 
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    );
    errcode_try("clEnqueueReadBufferRect()", errcode)
}

/// Enqueues a write from host memory, `data`, to device memory referred to by
/// `buffer`.
pub fn enqueue_write_buffer<T: OclNum, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            buffer: &Mem, 
            block: bool,
            offset: usize,
            data: &[T],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) =
        try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueWriteBuffer(
        command_queue.as_ptr(),
        buffer.as_ptr(),
        block as cl_uint,
        offset,
        (data.len() * mem::size_of::<T>()) as size_t,
        data.as_ptr() as cl_mem,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueWriteBuffer()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to write from a rectangular region from host memory to a buffer object.
///
/// ## Official Documentation
///
/// [SDK - clEnqueueWriteBufferRect]
///
/// [SDK - clEnqueueWriteBufferRect]: https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueWriteBufferRect.html
///
pub fn enqueue_write_buffer_rect<T: OclNum, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            buffer: &Mem, 
            block: bool,
            buffer_origin: [usize; 3],
            host_origin: [usize; 3],
            region: [usize; 3],
            buffer_row_pitch: usize,
            buffer_slc_pitch: usize,
            host_row_pitch: usize,
            host_slc_pitch: usize,
            data: &[T],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
    ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueWriteBufferRect(
        command_queue.as_ptr(), 
        buffer.as_ptr(), 
        block as cl_uint,
        &buffer_origin as *const _ as *const usize,
        &host_origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        buffer_row_pitch,
        buffer_slc_pitch,
        host_row_pitch,
        host_slc_pitch,
        data.as_ptr() as cl_mem, 
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueWriteBufferRect()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to fill a buffer object with a pattern of a given pattern size.
///
/// ### Pattern (from [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueFillBuffer.html))
///
/// A pointer to the data pattern of size pattern_size in bytes. pattern will be
/// used to fill a region in buffer starting at offset and is size bytes in size.
/// The data pattern must be a scalar or vector integer or floating-point data
/// type. For example, if buffer is to be filled with a pattern of float4 values,
/// then pattern will be a pointer to a cl_float4 value and pattern_size will be
/// sizeof(cl_float4). The maximum value of pattern_size is the size of the
/// largest integer or floating-point vector data type supported by the OpenCL
/// device. The memory associated with pattern can be reused or freed after the
/// function returns.
///
pub fn enqueue_fill_buffer<T, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            buffer: &Mem,
            pattern: &[T],
            pattern_size: usize,
            offset: usize,
            size: usize,
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueFillBuffer(
        command_queue.as_ptr(),
        buffer.as_ptr(), 
        &pattern as *const _ as *const c_void, 
        pattern_size,
        offset,
        size,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueFillBuffer()", errcode)
}

/// [UNTESTED]
/// Copies the contents of one buffer to another.
#[allow(dead_code)]
pub fn enqueue_copy_buffer<T: OclNum>(
            command_queue: &CommandQueue,
            src_buffer: &Mem,
            dst_buffer: &Mem,
            src_offset: usize,
            dst_offset: usize,
            len_copy_bytes: usize,
        ) -> OclResult<()> 
{
    let errcode = unsafe {
        cl_h::clEnqueueCopyBuffer(
        command_queue.as_ptr(),
        src_buffer.as_ptr(),
        dst_buffer.as_ptr(),
        src_offset,
        dst_offset,
        len_copy_bytes as usize,
        0,
        ptr::null(),
        ptr::null_mut(),
    ) };
    errcode_try("clEnqueueCopyBuffer()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to copy a rectangular region from a buffer object to
/// another buffer object.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueCopyBufferRect.html)
///
pub fn enqueue_copy_buffer_rect<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            src_buffer: &Mem, 
            dst_buffer: &Mem, 
            src_origin: [usize; 3],
            dst_origin: [usize; 3],
            region: [usize; 3],
            src_row_pitch: usize,
            src_slc_pitch: usize,
            dst_row_pitch: usize,
            dst_slc_pitch: usize,
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueCopyBufferRect(
        command_queue.as_ptr(), 
        src_buffer.as_ptr(), 
        dst_buffer.as_ptr(), 
        &src_origin as *const _ as *const usize,
        &dst_origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        src_row_pitch,
        src_slc_pitch,
        dst_row_pitch,
        dst_slc_pitch,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueCopyBufferRect()", errcode)
}


/// [UNTESTED] 
/// Reads an image from device to host memory.
///
/// ### Safety
///
/// Caller must ensure that `data` lives until the read is complete. Use
/// `new_event` to monitor it (use [`core::EventList::last_clone`] if passing
/// an event list as `new_event`).
///
/// [`core::EventList::get_clone`]: http://doc.cogciprocate.com/ocl/core/struct.EventList.html#method.last_clone
///
// pub unsafe fn enqueue_read_image<T, E: ClEventPtrNew>(
pub unsafe fn enqueue_read_image<T, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            image: &Mem,
            block: bool,
            origin: [usize; 3],
            region: [usize; 3],
            row_pitch: usize,
            slc_pitch: usize,
            data: &mut [T],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = cl_h::clEnqueueReadImage(
        command_queue.as_ptr(),
        image.as_ptr(),
        block as cl_uint,
        &origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        row_pitch,
        slc_pitch,
        data.as_ptr() as cl_mem,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    );
    errcode_try("clEnqueueReadImage()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to write to an image or image array object from host memory.
///
/// TODO: Size check (rather than leaving it to API).
pub fn enqueue_write_image<T, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            image: &Mem,
            block: bool,
            origin: [usize; 3],
            region: [usize; 3],
            input_row_pitch: usize,
            input_slc_pitch: usize,
            data: &[T],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueWriteImage(
        command_queue.as_ptr(),
        image.as_ptr(),
        block as cl_uint,
        &origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        input_row_pitch,
        input_slc_pitch,
        data.as_ptr() as cl_mem,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueWriteImage()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to fill an image object with a specified color.
///
/// ### Fill Color (from [SDK docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueFillImage.html)
///
/// The fill color. The fill color is a four component RGBA floating-point color
/// value if the image channel data type is not an unnormalized signed and
/// unsigned integer type, is a four component signed integer value if the image
/// channel data type is an unnormalized signed integer type and is a four
/// component unsigned integer value if the image channel data type is an
/// unormalized unsigned integer type. The fill color will be converted to the
/// appropriate image channel format and order associated with image.
///
/// TODO: Trait constraints for `T`. Presumably it should be 32bits? Testing needed.
pub fn enqueue_fill_image<T, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            image: &Mem,
            fill_color: [T; 4],
            origin: [usize; 3],
            region: [usize; 3],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueFillImage(
        command_queue.as_ptr(),
        image.as_ptr(), 
        &fill_color as *const _ as *const c_void, 
        &origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueFillImage()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to copy image objects.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueCopyImage.html)
pub fn enqueue_copy_image<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            src_image: &Mem,
            dst_image: &Mem,
            src_origin: [usize; 3],
            dst_origin: [usize; 3],
            region: [usize; 3],
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueCopyImage(
        command_queue.as_ptr(),
        src_image.as_ptr(),
        dst_image.as_ptr(),
        &src_origin as *const _ as *const usize,
        &dst_origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueCopyImage()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to copy an image object to a buffer object.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueCopyImageToBuffer.html)
pub fn enqueue_copy_image_to_buffer<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            src_image: &Mem,
            dst_buffer: &Mem,
            src_origin: [usize; 3],
            region: [usize; 3],
            dst_offset: usize,
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueCopyImageToBuffer(
        command_queue.as_ptr(),
        src_image.as_ptr(),
        dst_buffer.as_ptr(),
        &src_origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        dst_offset,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueCopyImageToBuffer()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to copy a buffer object to an image object.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueCopyBufferToImage.html)
pub fn enqueue_copy_buffer_to_image<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            src_buffer: &Mem,
            dst_image: &Mem,
            src_offset: usize,
            dst_origin: [usize; 3],
            region: [usize; 3],            
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let errcode = unsafe { cl_h::clEnqueueCopyBufferToImage(
        command_queue.as_ptr(),
        src_buffer.as_ptr(),
        dst_image.as_ptr(),
        src_offset,
        &dst_origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueCopyBufferToImage()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to map a region of the buffer object given by `buffer` into
/// the host address space and returns a pointer to this mapped region.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueMapBuffer.html)
///
/// ### Stability
///
/// This function will eventually return a safe wrapper for the mapped host
/// memory. Until then, just create a `Vec` from the returned pointer using
/// `size` / size_of::<T>() as the length and capacity.
///
/// ### Safety
///
/// Caller must ensure that the returned pointer is not used until the map is complete. Use
/// `new_event` to monitor it. [TEMPORARY] It also must be ensured that memory referred to by the returned pointer is not dropped, reused, or otherwise interfered with until `enqueue_unmap_mem_object` is called.
///
/// 
/// TODO: Return a new wrapped type representing the newly mapped memory.
///
/// [`EventList::get_clone`]: http://doc.cogciprocate.com/ocl/struct.EventList.html#method.last_clone
///
///
pub unsafe fn enqueue_map_buffer<T, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            buffer: &Mem,
            block: bool,
            map_flags: MapFlags,
            offset: usize,
            size: usize,         
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<*mut c_void> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) =
        try!(resolve_event_ptrs(wait_list, new_event));
    let mut errcode = 0i32;

    let mapped_ptr = cl_h::clEnqueueMapBuffer(
        command_queue.as_ptr(),
        buffer.as_ptr(),
        block as cl_uint,
        map_flags.bits(),
        offset,
        size,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
        &mut errcode,
    );
    try!(errcode_try("clEnqueueMapBuffer()", errcode));

    Ok(mapped_ptr)
}

/// [UNTESTED]
/// Enqueues a command to map a region of the image object given by `image` into
/// the host address space and returns a pointer to this mapped region.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueMapBuffer.html)
///
/// ### Stability
///
/// This function will eventually return a safe wrapper for the mapped host
/// memory. Until then, just create a `Vec` from the returned pointer using
/// `size` / size_of::<T>() as the length and capacity.
///
/// ### Safety
///
/// Caller must ensure that the returned pointer is not used until the map is complete. Use
/// `new_event` to monitor it. [TEMPORARY] It also must be ensured that memory referred to by the returned pointer is not dropped, reused, or otherwise interfered with until `enqueue_unmap_mem_object` is called.
///
/// 
/// TODO: Return a new wrapped type representing the newly mapped memory.
///
/// [`EventList::get_clone`]: http://doc.cogciprocate.com/ocl/struct.EventList.html#method.last_clone
///
///
pub unsafe fn enqueue_map_image<T, E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            image: &Mem,
            block: bool,
            map_flags: MapFlags,
            origin: [usize; 3],
            region: [usize; 3],
            row_pitch: usize,
            slc_pitch: usize,        
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<*mut c_void> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) =
        try!(resolve_event_ptrs(wait_list, new_event));
    let mut errcode = 0i32;

    let mapped_ptr = cl_h::clEnqueueMapImage(
        command_queue.as_ptr(),
        image.as_ptr(),
        block as cl_uint,
        map_flags.bits(),
        &origin as *const _ as *const usize,
        &region as *const _ as *const usize,
        row_pitch,
        slc_pitch,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
        &mut errcode,
    );
    try!(errcode_try("clEnqueueMapImage()", errcode));

    Ok(mapped_ptr)
}

/// [UNTESTED]
/// Enqueues a command to unmap a previously mapped region of a memory object.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueUnmapMemObject.html)
///
/// ### Stability
///
/// This function will eventually accept a safe wrapper of some sort for the
/// mapped host memory rather than a pointer.
///
pub fn enqueue_unmap_mem_object<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            memobj: &Mem,
            mapped_ptr: *mut c_void,
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> {
    let (wait_list_len, wait_list_ptr, new_event_ptr) =
        try!(resolve_event_ptrs(wait_list, new_event));
    
    let errcode = unsafe { cl_h::clEnqueueUnmapMemObject(
        command_queue.as_ptr(),
        memobj.as_ptr(),
        mapped_ptr,
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueUnmapMemObject()", errcode)
}

/// [UNTESTED]
/// Enqueues a command to indicate which device a set of memory objects should
/// be associated with.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueMigrateMemObjects.html)
pub fn enqueue_migrate_mem_objects<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            num_mem_objects: u32,
            mem_objects: &[Mem],
            flags: MemMigrationFlags,           
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) 
        = try!(resolve_event_ptrs(wait_list, new_event));

    let mem_ptr_list: Vec<cl_mem> = mem_objects.iter()
        .map(|ref mem_obj| unsafe { mem_obj.as_ptr() } ).collect();

    let errcode = unsafe { cl_h::clEnqueueMigrateMemObjects(
        command_queue.as_ptr(),
        num_mem_objects,
        mem_ptr_list.as_ptr() as *const _ as *const cl_mem,
        flags.bits(),
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueMigrateMemObjects()", errcode)
}

/// Enqueues a command to execute a kernel on a device.
///
/// # Stability
/// 
/// Work dimension/offset sizes *may* eventually be wrapped up in specialized types.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueNDRangeKernel.html)
pub fn enqueue_kernel<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            kernel: &Kernel,
            work_dims: u32,
            global_work_offset: Option<[usize; 3]>,
            global_work_dims: [usize; 3],
            local_work_dims: Option<[usize; 3]>,
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
            kernel_name: Option<&str>
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_ptrs(wait_list, new_event));
    let gwo = resolve_work_dims(&global_work_offset);
    let gws = &global_work_dims as *const size_t;
    let lws = resolve_work_dims(&local_work_dims);

    unsafe {
        let errcode = cl_h::clEnqueueNDRangeKernel(
            command_queue.as_ptr(),
            kernel.as_ptr() as cl_kernel,
            work_dims,
            gwo,
            gws,
            lws,
            wait_list_len,
            wait_list_ptr,
            new_event_ptr,
        );

        let errcode_pre = format!("clEnqueueNDRangeKernel('{}'):", kernel_name.unwrap_or(""));
        errcode_try(&errcode_pre, errcode)
    }
}

/// [UNTESTED] Enqueues a command to execute a kernel on a device.
///
/// The kernel is executed using a single work-item.
///
/// From [SDK]: clEnqueueTask is equivalent to calling clEnqueueNDRangeKernel 
/// with work_dim = 1, global_work_offset = NULL, global_work_size[0] set to 1,
/// and local_work_size[0] set to 1.
///
/// [SDK]: https://www.khronos.org/registry/cl/sdk/1.0/docs/man/xhtml/clEnqueueTask.html
///
pub fn enqueue_task<E: ClEventPtrNew>(
            command_queue: &CommandQueue,
            kernel: &Kernel,
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
            kernel_name: Option<&str>
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) = 
        try!(resolve_event_ptrs(wait_list, new_event));
    
    let errcode = unsafe { cl_h::clEnqueueTask(
            command_queue.as_ptr(),
            kernel.as_ptr() as cl_kernel,
            wait_list_len,
            wait_list_ptr,
            new_event_ptr,
    ) };
    let errcode_pre = format!("clEnqueueTask('{}'):", kernel_name.unwrap_or(""));
    errcode_try(&errcode_pre, errcode)    
}

/// [UNIMPLEMENTED][PLACEHOLDER]
pub fn enqueue_native_kernel() -> OclResult<()> {
    // cl_h::clEnqueueNativeKernel(command_queue: cl_command_queue,
    //                          user_func: extern fn (*mut c_void),
    //                          args: *mut c_void,
    //                          cb_args: size_t,
    //                          num_mem_objects: cl_uint,
    //                          mem_list: *const cl_mem,
    //                          args_mem_loc: *const *const c_void,
    //                          num_events_in_wait_list: cl_uint,
    //                          event_wait_list: *const cl_event,
    //                          event: *mut cl_event) -> cl_int;
    unimplemented!();
}

/// [UNTESTED]
/// Enqueues a marker command which waits for either a list of events to
/// complete, or all previously enqueued commands to complete.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueMarkerWithWaitList.html)
pub fn enqueue_marker_with_wait_list<E: ClEventPtrNew>(
            command_queue: &CommandQueue,          
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) =
        try!(resolve_event_ptrs(wait_list, new_event));
    
    let errcode = unsafe { cl_h::clEnqueueMarkerWithWaitList(
        command_queue.as_ptr(),
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueMarkerWithWaitList()", errcode)
}

/// [UNTESTED]
/// A synchronization point that enqueues a barrier operation.
///
/// [SDK Docs](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clEnqueueBarrierWithWaitList.html)
pub fn enqueue_barrier_with_wait_list<E: ClEventPtrNew>(
            command_queue: &CommandQueue,          
            wait_list: Option<&EventList>, 
            new_event: Option<&mut E>,
        ) -> OclResult<()> 
{
    let (wait_list_len, wait_list_ptr, new_event_ptr) =
        try!(resolve_event_ptrs(wait_list, new_event));
    
    let errcode = unsafe { cl_h::clEnqueueBarrierWithWaitList(
        command_queue.as_ptr(),
        wait_list_len,
        wait_list_ptr,
        new_event_ptr,
    ) };
    errcode_try("clEnqueueBarrierWithWaitList()", errcode)
}



/// [UNTESTED]
/// Returns the address of the extension function named by `func_name` for 
/// a given platform.
///
/// The pointer returned should be cast to a function pointer type matching the extension
/// function's definition defined in the appropriate extension specification and
/// header file. 
///
///
/// A non-NULL return value does
/// not guarantee that an extension function is actually supported by the
/// platform. The application must also make a corresponding query using
/// `ocl::core::get_platform_info(platform_core, CL_PLATFORM_EXTENSIONS, ... )` or
/// `ocl::core::get_device_info(device_core, CL_DEVICE_EXTENSIONS, ... )` 
/// to determine if an extension is supported by the OpenCL implementation.
///
/// [FIXME]: Update enum names above to the wrapped types.
///
/// # Errors
/// 
/// Returns an error if:
///
/// - `func_name` cannot be converted to a `CString`.
/// - The specified function does not exist for the implementation.
/// - 'platform' is not a valid platform.
///
// Extension function access
//
// Returns the extension function address for the given function name,
// or NULL if a valid function can not be found. The client must
// check to make sure the address is not NULL, before using or
// or calling the returned function address.
//
// A non-NULL return value for clGetExtensionFunctionAddressForPlatform does
// not guarantee that an extension function is actually supported by the
// platform. The application must also make a corresponding query using
// clGetPlatformInfo (platform, CL_PLATFORM_EXTENSIONS, ... ) or
// clGetDeviceInfo (device,CL_DEVICE_EXTENSIONS, ... ) to determine if an
// extension is supported by the OpenCL implementation.
// 
// [FIXME]: Return a generic that implements `Fn` (or `FnMut/Once`?).
// TODO: Create another function which will handle the second check described 
// above in addition to calling this.
pub unsafe fn get_extension_function_address_for_platform(platform: &PlatformId,
            func_name: &str) -> OclResult<*mut c_void> 
{
    let func_name_c = try!(CString::new(func_name));

    let ext_fn = cl_h::clGetExtensionFunctionAddressForPlatform(
        platform.as_ptr(),
        func_name_c.as_ptr(),
    );

    if ext_fn == 0 as *mut c_void { 
        OclError::err("The specified function does not exist for the implementation or 'platform' \
            is not a valid platform.")
    } else {
        Ok(ext_fn)
    }
}

//============================================================================
//============================================================================
//=========================== DERIVED FUNCTIONS ==============================
//============================================================================
//============================================================================
// MANY OF THESE NEED TO BE MORPHED INTO THE MORE GENERAL VERSIONS AND MOVED UP


/// Get the first platform.
pub fn get_first_platform() -> OclResult<PlatformId> {
    let platform_list = try!(get_platform_ids());

    if platform_list.len() == 0 {
        OclError::err("No platforms found!")
    } else {
        Ok(platform_list[0].clone())
    }
}

/// Creates, builds, and returns a new program pointer from `src_strings`.
///
/// TODO: Break out create and build parts into requisite functions then call
/// from here.
pub fn create_build_program<D: ClDeviceIdPtr>(
            context: &Context, 
            src_strings: &Vec<CString>,
            cmplr_opts: &CString,
            device_ids: &[D],
        ) -> OclResult<Program> 
{
    let program = try!(create_program_with_source(context, src_strings));
    try!(build_program(&program, device_ids, cmplr_opts, None, None));
    Ok(program)
}

/// Returns the maximum workgroup size of a device.
pub fn get_max_work_group_size<D: ClDeviceIdPtr>(device: &D) -> usize {
    let mut max_work_group_size: usize = 0;

    let errcode = unsafe { cl_h::clGetDeviceInfo(
        device.as_ptr(),
        cl_h::CL_DEVICE_MAX_WORK_GROUP_SIZE,
        mem::size_of::<usize>() as usize,
        &mut max_work_group_size as *mut _ as *mut c_void,
        ptr::null_mut(),
    ) };

    errcode_assert("clGetDeviceInfo", errcode);

    max_work_group_size
}

#[allow(dead_code)]
/// Blocks until an event is complete.
pub fn wait_for_event(event: &Event) {
    let errcode = unsafe {
        let event_ptr = *event.as_ptr_ref();
        cl_h::clWaitForEvents(1, &event_ptr)
    };
    errcode_assert("clWaitForEvents", errcode);
}

/// Returns the status of `event`.
pub fn get_event_status<'e, E: ClEventRef<'e>>(event: &'e E) -> OclResult<CommandExecutionStatus> {
    let mut status_int: cl_int = 0;

    let errcode = unsafe { 
        cl_h::clGetEventInfo(
            *event.as_ptr_ref(),
            cl_h::CL_EVENT_COMMAND_EXECUTION_STATUS,
            mem::size_of::<cl_int>(),
            &mut status_int as *mut _ as *mut c_void,
            ptr::null_mut(),
        )
    };
    try!(errcode_try("clGetEventInfo", errcode));

    CommandExecutionStatus::from_i32(status_int).ok_or(OclError::new("Error converting \
        'clGetEventInfo' status output."))
}

/// Verifies that the `context` is in fact a context object pointer.
///
/// # Assumptions
///
/// Some (most?/all?) OpenCL implementations do not correctly error if non-context pointers are passed. This function relies on the fact that passing the `CL_CONTEXT_DEVICES` as the `param_name` to `clGetContextInfo` will (at least on my AMD implementation) often return a huge result size if `context` is not actually a `cl_context` pointer due to the fact that it's reading from some random memory location on non-context structs. Also checks for zero because a context must have at least one device (true?). Should probably choose a value lower than 10kB because it seems unlikely any result would be that big but w/e.
///
/// [UPDATE]: This function may no longer be necessary now that the core pointers have wrappers but it still prevents a hard to track down bug so leaving it intact for now.
///
#[inline]
pub fn verify_context(context: &Context) -> OclResult<()> {
    // context_info(context, cl_h::CL_CONTEXT_REFERENCE_COUNT)
    if cfg!(release) {
        Ok(())
    } else {
        get_context_info(context, ContextInfo::Devices).and(Ok(()))
    }
}

//============================================================================
//============================================================================
//====================== Wow, you made it this far? ==========================
//============================================================================
//============================================================================



// /// [UNTESTED] Returns the platform name.
// ///
// /// TODO: DEPRICATE
// pub fn platform_name(platform: &PlatformId) -> OclResult<String> {
//     let info_result = try!(get_platform_info(Some(platform.clone()), PlatformInfo::Name));
//     Ok(info_result.into())
//     // println!("*** Platform Name ({}): {}", name, String::from_utf8(param_value).unwrap());
// }


// /// Returns a string containing requested information.
// ///
// /// Currently lazily assumes everything is a char[] and converts to a String. 
// /// Non-string info types need to be manually reconstructed from that. Yes this
// /// is retarded.
// ///
// /// [TODO (low priority)]: Needs to eventually be made more flexible and should return 
// /// an enum with a variant corresponding to the type of info requested. Could 
// /// alternatively return a generic type and just blindly cast to it.
// #[allow(dead_code, unused_variables)] 
// pub fn device_info(device_id: &DeviceId, info_type: cl_device_info) -> String {
//     let mut info_value_size: usize = 0;

//     let errcode = unsafe { 
//         cl_h::clGetDeviceInfo(
//             device_id.as_ptr(),
//             cl_h::CL_DEVICE_MAX_WORK_GROUP_SIZE,
//             mem::size_of::<usize>() as usize,
//             0 as cl_device_id,
//             &mut info_value_size as *mut usize,
//         ) 
//     }; 

//     errcode_assert("clGetDeviceInfo", errcode);

//     String::new()
// }




// /// Returns context information.
// ///
// /// [SDK Reference](https://www.khronos.org/registry/cl/sdk/1.2/docs/man/xhtml/clGetContextInfo.html)
// ///
// /// # Errors
// ///
// /// Returns an error result for all the reasons listed in the SDK in addition 
// /// to an additional error when called with `CL_CONTEXT_DEVICES` as described
// /// in in the `verify_context()` documentation below.
// ///
// /// TODO: Finish wiring up full functionality. Return a 'ContextInfo' enum result.
// pub fn context_info(context: &Context, request_param: cl_context_info)
//             -> OclResult<()> {
//     let mut result_size = 0;

//     // let request_param: cl_context_info = cl_h::CL_CONTEXT_PROPERTIES;
//     let errcode = unsafe { cl_h::clGetContextInfo(   
//         context.as_ptr(),
//         request_param,
//         0,(
//         0 as *mut c_void,
//         &mut result_size as *mut usize,
//     ) };
//     try!(errcode_try("clGetContextInfo", errcode));
//     // println!("context_info(): errcode: {}, result_size: {}", errcode, result_size);

//     let err_if_zero_result_size = request_param == cl_h::CL_CONTEXT_DEVICES;

//     if result_size > 10000 || (result_size == 0 && err_if_zero_result_size) {
//         return OclError::err("\n\nocl::core::context_info(): Possible invalid context detected. \n\
//             Context info result size is either '> 10k bytes' or '== 0'. Almost certainly an \n\
//             invalid context object. If not, please file an issue at: \n\
//             https://github.com/cogciprocate/ocl/issues.\n\n");
//     }

//     let mut result: Vec<u8> = iter::repeat(0).take(result_size).collect();

//     let errcode = unsafe { cl_h::clGetContextInfo(   
//         context.as_ptr(),
//         request_param,
//         result_size,
//         result.as_mut_ptr() as *mut c_void,
//         0 as *mut usize,
//     ) };
//     try!(errcode_try("clGetContextInfo", errcode));
//     // println!("context_info(): errcode: {}, result: {:?}", errcode, result);

//     Ok(())
// }