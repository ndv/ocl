#![allow(dead_code, unused_imports)]

use std::ptr;
use std::thread::Thread;
use std::sync::Arc;
use libc::c_void;

use futures;
use futures::{future, Future, Poll, Async};
use futures::sync::oneshot::{self, Sender};
use futures::task::{self, Task, UnparkEvent, EventSet};

use ffi::cl_event;
use functions;
use ::{Error as OclError, Result as OclResult, Event, UserEvent, OclPrm, MappedMem,
    MappedMemPtr, Mem, CommandQueue, CommandQueueInfo, CommandQueueInfoResult,
    CommandExecutionStatus, EventList};


#[cfg(feature = "future_event_callbacks")]
extern "C" fn _unpark_task(event_ptr: cl_event, event_status: i32, user_data: *mut c_void) {
    let _ = event_ptr;

    if event_status == CommandExecutionStatus::Complete as i32 && !user_data.is_null() {
        unsafe {
            println!("Unparking task via callback...");

            let task_ptr = user_data as *mut _ as *mut Task;
            let task = Box::from_raw(task_ptr);
            (*task).unpark();
        }
    } else {
        panic!("Wake up user data is null or event is not complete.");
    }
}


pub struct EventListTrigger {
    wait_events: EventList,
    completion_event: UserEvent,
    callback_is_set: bool,
}


pub struct EventTrigger {
    wait_event: Event,
    completion_event: UserEvent,
    callback_is_set: bool,
}

impl EventTrigger {
    pub fn new(wait_event: Event, completion_event: UserEvent) -> EventTrigger {
        EventTrigger {
            wait_event: wait_event,
            completion_event: completion_event ,
            callback_is_set: false,
        }
    }
}


pub struct FutureMappedMem<T: OclPrm> {
    ptr: MappedMemPtr<T>,
    len: usize,
    map_event: Event,
    unmap_event: Option<UserEvent>,
    buffer: Option<Mem>,
    queue: Option<CommandQueue>,
    callback_is_set: bool,

}

impl<T: OclPrm> FutureMappedMem<T> {
    pub unsafe fn new(ptr: *mut T, len: usize, map_event: Event, buffer: Mem, queue: CommandQueue)
            -> FutureMappedMem<T>
    {
        FutureMappedMem {
            ptr: MappedMemPtr::new(ptr),
            len: len,
            map_event: map_event,
            unmap_event: None,
            buffer: Some(buffer),
            queue: Some(queue),
            callback_is_set: false,
        }
    }

    pub fn create_unmap_event(&mut self) -> OclResult<&mut UserEvent> {
        if let Some(ref queue) = self.queue {
            let context = match functions::get_command_queue_info(queue,
                    CommandQueueInfo::Context)
            {
                CommandQueueInfoResult::Context(ctx) => ctx,
                CommandQueueInfoResult::Error(err) => return Err(*err),
                _ => unreachable!(),
            };

            match UserEvent::new(&context) {
                Ok(uev) => {
                    self.unmap_event = Some(uev);
                    Ok(self.unmap_event.as_mut().unwrap())
                }
                Err(err) => Err(err)
            }
        } else {
            Err("FutureMappedMem::create_unmap_event: No queue found!".into())
        }
    }

    pub fn to_mapped_mem(&mut self) -> OclResult<MappedMem<T>> {
        match self.buffer.take().map(|buf| self.queue.take().map(|qu| (buf, qu))) {
            Some(Some((buffer, queue))) => {
                unsafe { Ok(MappedMem::new(self.ptr.as_ptr(), self.len,
                    self.unmap_event.take(), buffer, queue )) }
            },
            _ => Err("FutureMappedMem::create_unmap_event: No queue and/or buffer found!".into()),
        }
    }

    /// Returns the unmap event if it has been created.
    #[inline]
    pub fn get_unmap_event(&self) -> Option<&UserEvent> {
        self.unmap_event.as_ref()
    }
}

/// Polling implementation.
#[cfg(not(feature = "future_event_callbacks"))]
impl<T: OclPrm> Future for FutureMappedMem<T> {
    type Item = MappedMem<T>;
    type Error = OclError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        println!("Polling FutureMappedMem...");

        loop {
            match self.map_event.is_complete() {
                Ok(true) => return self.to_mapped_mem().map(|mm| Async::Ready(mm)),
                Ok(false) => {
                    task::park();
                    continue;
                },
                Err(err) => return Err(err),
            };
        }
    }
}

#[cfg(feature = "future_event_callbacks")]
impl<T> Future for FutureMappedMem<T> where T: OclPrm + 'static {
    type Item = MappedMem<T>;
    type Error = OclError;

    fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
        println!("Polling FutureMappedMem...");

        match self.map_event.is_complete() {
            Ok(true) => {
                if !self.callback_is_set {
                    println!("Task completed on first poll.");
                } else {
                    println!("Unsetting callback...");
                    unsafe { self.map_event.set_callback(None, ptr::null_mut())?; }
                    self.callback_is_set = false;
                }

                return self.to_mapped_mem().map(|mm| Async::Ready(mm));
            }
            Ok(false) => {
                if !self.callback_is_set {
                    let task_box = Box::new(task::park());
                    let task_ptr = Box::into_raw(task_box) as *mut _ as *mut c_void;
                    println!("Setting callback...");
                    unsafe { self.map_event.set_callback(Some(_unpark_task), task_ptr)?; };
                    self.callback_is_set = true;
                }

                return Ok(Async::NotReady)
            },
            Err(err) => return Err(err),
        }
    }
}

unsafe impl<T: OclPrm> Send for FutureMappedMem<T> {}
unsafe impl<T: OclPrm> Sync for FutureMappedMem<T> {}




//#############################################################################
//#############################################################################
//########################### FAILED EXPERIMENTS ##############################
//#############################################################################
//#############################################################################

// #[cfg(feature = "future_event_callbacks")]
// extern "C" fn _unpark_task<T: OclPrm>(event_ptr: ffi::cl_event, event_status: i32,
//         user_data: *mut c_void)
// {
//     // let (_, _, _) = (event_ptr, event_status, user_data);
//     let _ = event_ptr;

//     if event_status == CommandExecutionStatus::Complete as i32 && !user_data.is_null() {
//         // let future = user_data as *mut _ as *mut FutureMappedMem<T>;
//         // let task_ptr = user_data as *mut _ as *mut Arc<Task>;
//         let tx_ptr = user_data as *mut _ as *mut Sender<T>;

//         unsafe {
//             println!("Unparking task via callback...");

//             // (*task).unpark();
//             // if (*task).is_current() {
//             //     (*task).unpark();
//             // } else {
//             //     panic!("futures::_unpark_task: Task is not current.");
//             // }

//             // (*future).poll();

//             // self.to_mapped_mem().map(|mm| Async::Ready(mm))

//             // (*future).task.as_ref().unwrap().unpark();

//             // let task = Arc::from_raw(task_ptr);
//             // task.unpark();

//             let tx = Box::from_raw(tx_ptr);
//             tx.complete(Default::default());
//         }
//     } else {
//         panic!("Wake up user data is null or event is not complete.");
//     }
// }


// #[cfg(feature = "future_event_callbacks")]
// extern "C" fn _dummy(_: ffi::cl_event, _: i32, _: *mut c_void) {}


// #[cfg(feature = "future_event_callbacks")]
// impl<T: OclPrm> Future for FutureMappedMem<T> {
//     type Item = MappedMem<T>;
//     type Error = OclError;

//     fn poll(&mut self) -> Poll<Self::Item, Self::Error> {
//         println!("Polling FutureMappedMem...");

//         loop {
//             match self.map_event.is_complete() {
//                 Ok(true) => {
//                     if self.task.is_none() {
//                         println!("Task completed on first poll.");
//                         // return self.to_mapped_mem().map(|mm| Async::Ready(mm));
//                     } else {
//                         println!("Unsetting callback...");
//                         unsafe { self.map_event.set_callback::<T>(Some(_dummy), None)?; }
//                         self.task = None;
//                         // println!("Task complete but waiting on callback.");
//                         // continue;
//                     }

//                     return self.to_mapped_mem().map(|mm| Async::Ready(mm));
//                 }
//                 Ok(false) => {
//                     if self.task.is_none() {
//                         println!("Task incomplete...");
//                         // self.task = Some(Arc::new(task::park()));

//                         // let unpark = Arc::new(ThreadUnpark::new(thread::current()));

//                         unsafe {
//                             println!("Setting event callback...");

//                             // // [`FutureMappedMem<T>`]:
//                             // let self_ptr = self as *mut _ as *mut c_void;
//                             // self.map_event.set_callback_with_ptr(Some(_unpark_task::<T>), self_ptr)?;

//                             // // [`Arc<Task>`]:
//                             // let task = Arc::into_raw(self.task.clone().unwrap()) as *mut _ as *mut c_void;
//                             // self.map_event.set_callback_with_ptr(Some(_unpark_task::<T>), task)?;

//                             let (tx, rx) = oneshot::channel::<()>();
//                             let tx_ptr = Box::into_raw(Box::new(tx)) as *mut _ as *mut c_void;
//                             self.map_event.set_callback_with_ptr(Some(_unpark_task::<T>), tx_ptr)?;

//                             // rx.wait().unwrap();
//                             // let mm = self.to_mapped_mem().map(|mm| Async::Ready(mm))?;
//                             // return Ok(rx.and_then(|| mm));
//                         }

//                         // return self.to_mapped_mem().map(|mm| Async::Ready(mm))

//                         // self.task = Some(task::park());
//                         // continue;
//                         panic!("Whatever.");
//                     } else {
//                         println!("Task incomplete, already parked.");
//                     }

//                     // continue;
//                     return Ok(Async::NotReady);
//                 },
//                 Err(err) => return Err(err),
//             }
//         }
//     }
// }