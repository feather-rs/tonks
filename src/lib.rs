#[macro_use]
extern crate derivative;

mod scheduler;

use crate::scheduler::OneshotTuple;
pub use scheduler::{Oneshot, Scheduler, SchedulerBuilder};
use shred::ResourceId;
use shred::SystemData;

#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
#[repr(u8)]
pub enum EnumExecutionStrategy {
    Relaxed,
    BeforeDependents,
    Immediate,
}

pub trait ExecutionStrategy {
    fn strategy() -> EnumExecutionStrategy;
}

pub trait OneshotExecutionStrategy: ExecutionStrategy {}

pub struct Relaxed;
impl ExecutionStrategy for Relaxed {
    fn strategy() -> EnumExecutionStrategy {
        EnumExecutionStrategy::Relaxed
    }
}
impl OneshotExecutionStrategy for Relaxed {}

pub struct BeforeDependents;
impl ExecutionStrategy for BeforeDependents {
    fn strategy() -> EnumExecutionStrategy {
        EnumExecutionStrategy::BeforeDependents
    }
}
impl OneshotExecutionStrategy for BeforeDependents {}

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

pub trait Event: Send + Sync {}

impl<T> Event for T where T: Send + Sync {}
