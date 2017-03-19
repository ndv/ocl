//! Asynchronous read, write, and map tests.


use std::thread;
use futures::{Future, BoxFuture};
use ::{Platform, Device, Context, Queue, Program, Kernel, Event, Buffer, RwVec};
use ::traits::{IntoRawList};
use ::async::{Error as AsyncError};
use ::flags::{MemFlags, MapFlags, CommandQueueProperties};
use ::prm::Int4;
use ::ffi::{cl_event, c_void};

// Size of buffers and kernel work size:
//
// NOTE: Intel platform drivers may intermittently crash and error with
// `DEVICE_NOT_AVAILABLE` if this number is too low. Use AMD drivers.
const WORK_SIZE: usize = 1 << 20;

// Initial value and addend for this example:
const INIT_VAL: i32 = 50;
const SCALAR_ADDEND: i32 = 100;

// The number of tasks to run concurrently.
const TASK_ITERS: i32 = 8;

const PRINT: bool = false;


// A kernel that makes a career out of adding values.
pub static KERN_SRC: &'static str = r#"
    __kernel void add_slowly(
        __global int4* in,
        __private int addend,
        __global int4* out)
    {
        uint const idx = get_global_id(0);

        float4 const inflated_val = (float4)(addend) * (float4)(255.0);
        int4 sum = (int4)(0);

        for (int i = 0; i < addend; i++) {
            sum += convert_int4((inflated_val / (float4)(255.0)) / (float4)(addend));
        }

        out[idx] = in[idx] + sum;
    }
"#;



/// 0. Fill-Junk
/// ============
///
/// Fill buffer with -999's just to ensure the upcoming write misses nothing:
pub fn fill_junk(src_buf: &Buffer<Int4>, common_queue: &Queue,
        kernel_event: Option<&Event>,
        write_init_event: Option<&Event>,
        fill_event: &mut Option<Event>,
        task_iter: i32)
{
    // These just print status messages...
    extern "C" fn _print_starting(_: cl_event, _: i32, task_iter : *mut c_void) {
        if PRINT { println!("* Fill starting        \t(iter: {}) ...", task_iter as usize); }
    }
    extern "C" fn _print_complete(_: cl_event, _: i32, task_iter : *mut c_void) {
        if PRINT { println!("* Fill complete        \t(iter: {})", task_iter as usize); }
    }

    // Clear the wait list and push the previous iteration's kernel event
    // and the previous iteration's write init (unmap) event if they are set.
    let wait_list = [&kernel_event, &write_init_event].into_raw_list();

    // Create a marker so we can print the status message:
    let fill_wait_marker = wait_list.to_marker(&common_queue).unwrap();

    if let Some(ref marker) = fill_wait_marker {
        unsafe { marker.set_callback(_print_starting, task_iter as *mut c_void).unwrap(); }
    } else {
        _print_starting(0 as cl_event, 0, task_iter as *mut c_void);
    }

    *fill_event = Some(Event::empty());

    src_buf.cmd().fill(Int4::new(-999, -999, -999, -999), None)
        .queue(common_queue)
        .ewait(&wait_list)
        .enew_opt(fill_event.as_mut())
        .enq().unwrap();

    unsafe { fill_event.as_ref().unwrap()
        .set_callback(_print_complete, task_iter as *mut c_void).unwrap(); }
}


/// 1. Write-Init
/// =================
///
/// Map the buffer and write 50's to the entire buffer, then
/// unmap to actually move data to the device. The `map` will use
/// the common queue and the `unmap` will automatically use the
/// dedicated queue passed to the buffer during creation (unless we
/// specify otherwise).
pub fn write_init(src_buf: &Buffer<Int4>, rw_vec: &RwVec<Int4>, common_queue: &Queue,
        write_init_unmap_queue: &Queue,
        fill_event: Option<&Event>,
        verify_init_event: Option<&Event>,
        write_init_event: &mut Option<Event>,
        write_val: i32, task_iter: i32)
        -> BoxFuture<i32, AsyncError>
{
    extern "C" fn _write_complete(_: cl_event, _: i32, task_iter : *mut c_void) {
        if PRINT { println!("* Write init complete  \t(iter: {})", task_iter as usize); }
    }

    // Clear the wait list and push the previous iteration's verify init event
    // and the current iteration's fill event if they are set.
    let wait_list = [&verify_init_event, &fill_event].into_raw_list();

    // let mut future_write_data = src_buf.cmd().map()
    //     .queue(common_queue)
    //     .flags(MapFlags::new().write_invalidate_region())
    //     .ewait(&wait_list)
    //     .enq_async().unwrap();

    let mut future_write_data = src_buf.cmd().write(rw_vec)
        .queue(common_queue)
        // .flags(MapFlags::new().write_invalidate_region())
        .ewait(&wait_list)
        .enq_async().unwrap();

    // Set the write unmap completion event which will be set to complete
    // (triggered) after the CPU-side processing is complete and the data is
    // transferred to the device:
    *write_init_event = Some(future_write_data.create_drop_event(&write_init_unmap_queue)
        .unwrap().clone());

    unsafe { write_init_event.as_ref().unwrap().set_callback(_write_complete,
        task_iter as *mut c_void).unwrap(); }

    future_write_data.and_then(move |mut data| {
        if PRINT { println!("* Write init starting  \t(iter: {}) ...", task_iter); }

        for val in data.iter_mut() {
            *val = Int4::new(write_val, write_val, write_val, write_val);
        }

        // // Normally we could just let `data` (a `MemMap`) fall out of
        // // scope and it would unmap itself. Since we need to specify a
        // // special dedicated queue to avoid deadlocks in this case, we
        // // call it explicitly.
        // data.unmap().queue(&write_init_unmap_queue).enq()?;

        Ok(task_iter)
    }).boxed()
}


/// 2. Verify-Init
/// ===================
///
/// Read results and verify that the initial mapped write has completed
/// successfully. This will use the common queue for the read and a dedicated
/// queue for the verification completion event (used to signal the next
/// command in the chain).
pub fn verify_init(src_buf: &Buffer<Int4>, rw_vec: &RwVec<Int4>, common_queue: &Queue,
        verify_init_queue: &Queue,
        write_init_event: Option<&Event>,
        verify_init_event: &mut Option<Event>,
        correct_val: i32, task_iter: i32)
        -> BoxFuture<i32, AsyncError>
{
    extern "C" fn _verify_starting(_: cl_event, _: i32, task_iter : *mut c_void) {
        if PRINT { println!("* Verify init starting \t(iter: {}) ...", task_iter as usize); }
    }

    // Clear the wait list and push the previous iteration's read verify
    // completion event (if it exists) and the current iteration's write unmap
    // event.
    let wait_list = [&verify_init_event.as_ref(), &write_init_event].into_raw_list();

    let mut future_read_data = src_buf.cmd().read(rw_vec)
        .queue(common_queue)
        .ewait(&wait_list)
        .enq_async().unwrap();

    // Attach a status message printing callback to what approximates the
    // verify_init start-time event:
    unsafe { future_read_data.command_trigger_event().set_callback(
        _verify_starting, task_iter as *mut c_void).unwrap(); }

    // Create an empty event ready to hold the new verify_init event, overwriting any old one.
    *verify_init_event = Some(future_read_data.create_drop_event(verify_init_queue)
        .unwrap().clone());

    // The future which will actually verify the initial value:
    future_read_data.and_then(move |data| {
        let mut val_count = 0;

        for (idx, val) in data.iter().enumerate() {
            let cval = Int4::new(correct_val, correct_val, correct_val, correct_val);
            if *val != cval {
                return Err(format!("Verify init: Result value mismatch: {:?} != {:?} @ [{}]", val, cval, idx).into());
            }
            val_count += 1;
        }

        if PRINT { println!("* Verify init complete \t(iter: {})", task_iter); }

        Ok(val_count)
    }).boxed()
}


/// 3. Kernel-Add
/// =============
///
/// Enqueues a kernel which adds a value to each element in the input buffer.
///
/// The `Kernel complete ...` message is sometimes delayed slightly (a few
/// microseconds) due to the time it takes the callback to trigger.
pub fn kernel_add(kern: &Kernel, common_queue: &Queue,
        verify_add_event: Option<&Event>,
        write_init_event: Option<&Event>,
        kernel_event: &mut Option<Event>,
        task_iter: i32)
{
    // These just print status messages...
    extern "C" fn _print_starting(_: cl_event, _: i32, task_iter : *mut c_void) {
        if PRINT { println!("* Kernel starting      \t(iter: {}) ...", task_iter as usize); }
    }
    extern "C" fn _print_complete(_: cl_event, _: i32, task_iter : *mut c_void) {
        if PRINT { println!("* Kernel complete      \t(iter: {})", task_iter as usize); }
    }

    // Clear the wait list and push the previous iteration's read unmap event
    // and the current iteration's write unmap event if they are set.
    let wait_list = [&verify_add_event, &write_init_event].into_raw_list();

    // Create a marker so we can print the status message:
    let kernel_wait_marker = wait_list.to_marker(&common_queue).unwrap();

    // Attach a status message printing callback to what approximates the
    // kernel wait (start-time) event:
    unsafe { kernel_wait_marker.as_ref().unwrap()
        .set_callback(_print_starting, task_iter as *mut c_void).unwrap(); }

    // Create an empty event ready to hold the new kernel event, overwriting any old one.
    *kernel_event = Some(Event::empty());

    // Enqueues the kernel. Since we did not specify a default queue upon
    // creation (for no particular reason) we must specify it here. Also note
    // that the events that this kernel depends on are linked to the *unmap*,
    // not the map commands of the preceding read and writes.
    kern.cmd()
        .queue(common_queue)
        .ewait(&wait_list)
        .enew_opt(kernel_event.as_mut())
        .enq().unwrap();

    // Attach a status message printing callback to the kernel completion event:
    unsafe { kernel_event.as_ref().unwrap().set_callback(_print_complete,
        task_iter as *mut c_void).unwrap(); }
}


/// 4. Verify-Add
/// =================
///
/// Read results and verify that the write and kernel have both
/// completed successfully. The `map` will use the common queue and the
/// `unmap` will use a dedicated queue to avoid deadlocks.
///
/// This occasionally shows as having begun a few microseconds before the
/// kernel has completed but that's just due to the slight callback delay on
/// the kernel completion event.
pub fn verify_add(dst_buf: &Buffer<Int4>, rw_vec: &RwVec<Int4>, common_queue: &Queue,
        verify_add_unmap_queue: &Queue,
        wait_event: Option<&Event>,
        verify_add_event: &mut Option<Event>,
        correct_val: i32, task_iter: i32)
        -> BoxFuture<i32, AsyncError>
{
    extern "C" fn _verify_starting(_: cl_event, _: i32, task_iter : *mut c_void) {
        if PRINT { println!("* Verify add starting  \t(iter: {}) ...", task_iter as usize); }
    }

    // unsafe { wait_event.as_ref().unwrap()
    //     .set_callback(_verify_starting, task_iter as *mut c_void).unwrap(); }

    // let mut future_read_data = dst_buf.cmd().map()
    //     .queue(common_queue)
    //     .flags(MapFlags::new().read())
    //     .ewait_opt(wait_event)
    //     .enq_async().unwrap();

    let mut future_read_data = dst_buf.cmd().read(rw_vec)
        .queue(common_queue)
        .ewait_opt(wait_event)
        .enq_async().unwrap();

    // Attach a status message printing callback to what approximates the
    // verify_init start-time event:
    unsafe { future_read_data.command_trigger_event().set_callback(
        _verify_starting, task_iter as *mut c_void).unwrap(); }

    // // Set the read unmap completion event:
    // *verify_add_event = Some(future_read_data.create_unmap_event().unwrap().clone());

    // Create an empty event ready to hold the new verify_init event, overwriting any old one.
    *verify_add_event = Some(future_read_data.create_drop_event(&verify_add_unmap_queue)
        .unwrap().clone());

    future_read_data.and_then(move |data| {
        let mut val_count = 0;

        for (idx, val) in data.iter().enumerate() {
            let cval = Int4::splat(correct_val);
            if *val != cval {
                return Err(format!("Verify add: Result value mismatch: {:?} != {:?} @ [{}]", 
                    val, cval, idx).into());
            }
            val_count += 1;
        }

        if PRINT { println!("* Verify add complete  \t(iter: {})", task_iter); }

        Ok(val_count)
    }).boxed()
}


/// Main
/// ====
///
/// Repeatedly:
///   0. fills with garbage,
///   1. writes a start value,
///   2. verifies the write,
///   3. adds a value,
///   4. and verifies the sum.
///
#[test]
pub fn rw_vec() {
    let platform = Platform::default();
    println!("Platform: {}", platform.name());
    let device = Device::first(platform);
    println!("Device: {} {}", device.vendor(), device.name());

    let context = Context::builder()
        .platform(platform)
        .devices(device)
        .build().unwrap();

    // For unmap commands, the buffers will each use a dedicated queue to
    // avoid any chance of a deadlock. All other commands will use an
    // unordered common queue.
    let queue_flags = Some(CommandQueueProperties::new().out_of_order());
    let common_queue = Queue::new(&context, device, queue_flags).unwrap();
    let write_init_unmap_queue = Queue::new(&context, device, queue_flags).unwrap();
    let verify_init_queue = Queue::new(&context, device, queue_flags).unwrap();
    let verify_add_unmap_queue = Queue::new(&context, device, queue_flags).unwrap();

    // Allocating host memory allows the OpenCL runtime to use special pinned
    // memory which considerably improves the transfer performance of map
    // operations for devices that do not already use host memory (GPUs,
    // etc.). Adding read and write only specifiers also allows for other
    // optimizations.
    let src_buf_flags = MemFlags::new().alloc_host_ptr().read_only();
    let dst_buf_flags = MemFlags::new().alloc_host_ptr().write_only().host_read_only();

    // Create write and read buffers:
    let src_buf: Buffer<Int4> = Buffer::builder()
        .context(&context)
        .flags(src_buf_flags)
        .dims(WORK_SIZE)
        .build().unwrap();

    let dst_buf: Buffer<Int4> = Buffer::builder()
        .context(&context)
        .flags(dst_buf_flags)
        .dims(WORK_SIZE)
        .build().unwrap();

    // Create program and kernel:
    let program = Program::builder()
        .devices(device)
        .src(KERN_SRC)
        .build(&context).unwrap();

    let kern = Kernel::new("add_slowly", &program).unwrap()
        .gws(WORK_SIZE)
        .arg_buf(&src_buf)
        .arg_scl(SCALAR_ADDEND)
        .arg_buf(&dst_buf);

    // A lockable vector for reads and writes:
    let rw_vec: RwVec<Int4> = RwVec::from(vec![Default::default(); WORK_SIZE]);

    // A place to store our threads:
    let mut threads = Vec::with_capacity(TASK_ITERS as usize);

    // Our events for synchronization.
    let mut fill_event = None;
    let mut write_init_event = None;
    let mut verify_init_event: Option<Event> = None;
    let mut kernel_event = None;
    let mut verify_add_event = None;

    println!("Starting cycles ...");

    // Our main loop. Could run indefinitely if we had a stream of input.
    for task_iter in 0..TASK_ITERS {
        let ival = INIT_VAL + task_iter;
        let tval = ival + SCALAR_ADDEND;

        // 0. Fill-Junk
        // ============
        fill_junk(&src_buf, &common_queue,
            write_init_event.as_ref(),
            kernel_event.as_ref(),
            &mut fill_event,
            task_iter);

        // 1. Write-Init
        // ============
        let write_init = write_init(&src_buf, &rw_vec, &common_queue,
            &write_init_unmap_queue,
            fill_event.as_ref(),
            verify_init_event.as_ref(),
            &mut write_init_event,
            ival, task_iter);

        // 2. Verify-Init
        // ============
        let verify_init = verify_init(&src_buf, &rw_vec, &common_queue,
            &verify_init_queue,
            write_init_event.as_ref(),
            &mut verify_init_event,
            ival, task_iter);

        // 3. Kernel-Add
        // =============
        kernel_add(&kern, &common_queue,
            verify_add_event.as_ref(),
            write_init_event.as_ref(),
            &mut kernel_event,
            task_iter);

        // 4. Verify-Add
        // =================
        let verify_add = verify_add(&dst_buf, &rw_vec, &common_queue,
            &verify_add_unmap_queue,
            kernel_event.as_ref(),
            &mut verify_add_event,
            tval, task_iter);

        println!("All commands for iteration {} enqueued", task_iter);

        let task = write_init.join3(verify_init, verify_add);

        threads.push(thread::spawn(move || {
            task.wait().unwrap();
        }));
    }

    for thread in threads {
        thread.join().unwrap();
    }

    println!("All result values are correct!");
}