#[macro_use]
extern crate derivative;

mod event_handler;
mod scheduler;

pub use crate::event_handler::EventHandler;
use crate::scheduler::OneshotTuple;
pub use scheduler::{Oneshot, Scheduler, SchedulerBuilder};
use shred::ResourceId;
use shred::SystemData;
use std::any::Any;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EnumExecutionStrategy {
    Relaxed,
    BeforeDependents,
    Immediate,
}

pub trait ExecutionStrategy: private::Sealed {
    fn strategy() -> EnumExecutionStrategy;
}

pub trait OneshotExecutionStrategy: ExecutionStrategy + private::OneshotSealed {}

pub struct EndOfDispatch;
impl ExecutionStrategy for EndOfDispatch {
    fn strategy() -> EnumExecutionStrategy {
        EnumExecutionStrategy::Relaxed
    }
}
impl OneshotExecutionStrategy for EndOfDispatch {}

pub struct Next;
impl ExecutionStrategy for Next {
    fn strategy() -> EnumExecutionStrategy {
        EnumExecutionStrategy::BeforeDependents
    }
}
impl OneshotExecutionStrategy for Next {}

pub struct Immediate;
impl ExecutionStrategy for Immediate {
    fn strategy() -> EnumExecutionStrategy {
        EnumExecutionStrategy::Immediate
    }
}

pub trait System<'a>: Send + Sync {
    type SystemData: SystemData<'a>;
    type Oneshot: OneshotTuple;

    fn run(&self, data: Self::SystemData, oneshot: Self::Oneshot);

    fn reads(&self) -> Vec<ResourceId> {
        Self::SystemData::reads()
    }

    fn writes(&self) -> Vec<ResourceId> {
        Self::SystemData::writes()
    }
}

pub trait Event: Send + Sync + Any {}

impl<T> Event for T where T: Send + Sync + Any + 'static {}

mod private {
    use crate::{EndOfDispatch, Immediate, Next};

    pub trait Sealed {}

    impl Sealed for Immediate {}
    impl Sealed for Next {}
    impl Sealed for EndOfDispatch {}

    pub trait OneshotSealed {}
    impl OneshotSealed for Next {}
    impl OneshotSealed for EndOfDispatch {}
}
