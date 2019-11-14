//! Type-level query APIs as wrappers over Legion queries.

use crate::system::SystemCtx;
use crate::{ResourceId, Resources, SystemData, SystemDataOutput};
use legion::filter::EntityFilter;
use legion::query::{ChunkDataIter, ChunkViewIter, DefaultFilter, IntoQuery, View};
use legion::world::World;

pub struct Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
{
    inner: legion::query::Query<V, <V as DefaultFilter>::Filter>,
}

impl<'a, V> SystemData<'a> for Query<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync + 'a,
{
    type Output = PreparedQuery<'a, V, <V as DefaultFilter>::Filter>;

    fn prepare(&'a mut self) -> Self::Output {
        PreparedQuery {
            query: &mut self.inner,
        }
    }

    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    unsafe fn load_from_resources(_resources: &Resources, _ctx: SystemCtx, _world: &World) -> Self {
        Self { inner: V::query() }
    }
}

/// A `legion::World` wrapper which can be safely passed to systems.
pub struct PreparedWorld {
    world: *const World,
}

assert_impl_all!(World: Send, Sync);
unsafe impl Send for PreparedWorld {}
unsafe impl Sync for PreparedWorld {}

impl<'a> SystemData<'a> for PreparedWorld {
    type Output = &'a mut Self;

    fn prepare(&'a mut self) -> Self::Output {
        self
    }

    fn reads() -> Vec<ResourceId> {
        vec![]
    }

    fn writes() -> Vec<ResourceId> {
        vec![]
    }

    unsafe fn load_from_resources(_resources: &Resources, _ctx: SystemCtx, world: &World) -> Self {
        Self {
            world: world as *const _,
        }
    }
}

impl<'a> SystemDataOutput<'a> for &'a mut PreparedWorld {
    type SystemData = PreparedWorld;
}

/// A query which has been prepared for passing to a system.
pub struct PreparedQuery<'a, V, F>
where
    V: for<'v> View<'v>,
    F: EntityFilter,
{
    query: &'a mut legion::query::Query<V, F>,
}

impl<'a, V, F> PreparedQuery<'a, V, F>
where
    V: for<'v> View<'v>,
    F: EntityFilter,
{
    pub fn iter<'b, 'data>(
        &'b mut self,
        world: &'data mut PreparedWorld,
    ) -> ChunkDataIter<
        'data,
        V,
        ChunkViewIter<'data, 'b, V, F::ArchetypeFilter, F::ChunksetFilter, F::ChunkFilter>,
    > {
        unsafe { self.query.iter_unchecked(&*world.world) }
    }
}

impl<'a, V> SystemDataOutput<'a> for PreparedQuery<'a, V, <V as DefaultFilter>::Filter>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync,
{
    type SystemData = Query<V>;
}
