//! Stage of a scheduler. This works similar to the dispatcher in `shred`;
//! see https://github.com/slide-rs/shred/blob/master/src/dispatch/stage.rs
//! for more explanation.

use crate::scheduler::SystemId;
use arrayvec::ArrayVec;
use smallvec::SmallVec;

const MAX_SYSTEMS_PER_GROUP: usize = 5;

type GroupVec<T> = ArrayVec<[T; MAX_SYSTEMS_PER_GROUP]>;
type StageVec<T> = SmallVec<[T; 6]>;

#[derive(Default)]
pub struct Stage {
    groups: StageVec<GroupVec<SystemId>>,
}
