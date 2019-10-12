use crate::scheduler::run_now::RunNow;
use crate::scheduler::Context;
use crate::scheduler::{DynSystem, TaskMessage};
use crate::{ExecutionStrategy, OneshotExecutionStrategy, System};
use std::marker::PhantomData;
use std::mem;

/// Type which can be used to schedule one-shot systems for running.
///
/// Note that one-shot systems do not support `Immediate` execution
/// strategy.
pub struct Oneshot<S: OneshotExecutionStrategy> {
    /// Context of the system using this `Oneshot`.
    ctx: Context,
    _phantom: PhantomData<S>,
}

impl<S: OneshotExecutionStrategy> Oneshot<S> {
    /// Schedules a one-shot system for running. This system
    /// is guaranteed to be run during this dispatch and
    /// will respect the `ExecutionStrategy` associated
    /// with this type.
    pub fn schedule<'a, R: System<'a> + 'static>(&self, system: R) {
        let boxed: Box<dyn RunNow<'a>> = Box::new(system);
        // Transmute is safe because the lifetime parameter is used
        // only for the `SystemData`.
        let system =
            unsafe { mem::transmute::<Box<DynSystem<'a>>, Box<DynSystem<'static>>>(boxed) };
        let msg = TaskMessage::ScheduleOneshot {
            scheduling_system: self.ctx.id,
            system,
            strategy: <S as ExecutionStrategy>::strategy(),
        };
        self.ctx.sender.send(msg).unwrap();
    }
}

pub trait OneshotTuple {
    fn from_ctx(ctx: Context) -> Self;
}

impl OneshotTuple for () {
    fn from_ctx(_ctx: Context) -> Self {
        ()
    }
}

// Only implement for up to 3 types because
// there are only three different execution strategies.
impl<S> OneshotTuple for Oneshot<S>
where
    S: OneshotExecutionStrategy,
{
    fn from_ctx(ctx: Context) -> Self {
        Self {
            ctx,
            _phantom: PhantomData,
        }
    }
}

impl<S1, S2> OneshotTuple for (Oneshot<S1>, Oneshot<S2>)
where
    S1: OneshotExecutionStrategy,
    S2: OneshotExecutionStrategy,
{
    fn from_ctx(ctx: Context) -> Self {
        (
            Oneshot {
                ctx: ctx.clone(),
                _phantom: PhantomData,
            },
            Oneshot {
                ctx,
                _phantom: PhantomData,
            },
        )
    }
}

impl<S1, S2, S3> OneshotTuple for (Oneshot<S1>, Oneshot<S2>, Oneshot<S3>)
where
    S1: OneshotExecutionStrategy,
    S2: OneshotExecutionStrategy,
    S3: OneshotExecutionStrategy,
{
    fn from_ctx(ctx: Context) -> Self {
        (
            Oneshot {
                ctx: ctx.clone(),
                _phantom: PhantomData,
            },
            Oneshot {
                ctx: ctx.clone(),
                _phantom: PhantomData,
            },
            Oneshot {
                ctx,
                _phantom: PhantomData,
            },
        )
    }
}
