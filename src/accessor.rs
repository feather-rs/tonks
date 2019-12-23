use crate::system::SystemCtx;
use crate::{MacroData, PreparedWorld, ResourceId, Resources, SystemData, SystemDataOutput};
use legion::borrow::Ref;
use legion::entity::Entity;
use legion::query::{DefaultFilter, View};
use legion::storage::{Component, ComponentTypeId};
use legion::world::World;
use std::marker::PhantomData;

/// An entity accessor type which can be used to immutably access
/// arbitrary components of an entity.
pub struct EntityAccessor<'a> {
    entity: Entity,
    _phantom: PhantomData<&'a ()>,
}

impl<'a> EntityAccessor<'a> {
    /// Retrieves a component of this entity.
    pub fn get_component<'b, C: Component>(&self, world: &'b PreparedWorld) -> Option<Ref<'b, C>> {
        unsafe { &*world.world }.get_component(self.entity)
    }
}

/// System data which allows retrieval of `EntityAccessor`s for all entities
/// matching a `View`.
pub struct QueryAccessor<V> {
    _world: *const World,
    _phantom: PhantomData<V>,
}

unsafe impl<V> Send for QueryAccessor<V> {}
unsafe impl<V> Sync for QueryAccessor<V> {}

impl<V> QueryAccessor<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync,
{
    /// Retrieves an `EntityAccessor` for the given entity,
    /// or `None` if the entity does not fall within this `View`.
    pub fn find(&self, entity: Entity) -> Option<EntityAccessor> {
        // TODO: validate
        Some(EntityAccessor {
            entity,
            _phantom: PhantomData,
        })
    }
}

impl<'a, V> SystemData<'a> for QueryAccessor<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync + 'a,
{
    type Output = &'a Self;

    unsafe fn load_from_resources(
        _resources: &mut Resources,
        _ctx: SystemCtx,
        world: &World,
    ) -> Self {
        Self {
            _world: world as *const World,
            _phantom: PhantomData,
        }
    }

    fn resource_reads() -> Vec<ResourceId> {
        vec![]
    }

    fn resource_writes() -> Vec<ResourceId> {
        vec![]
    }

    fn component_reads() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn component_writes() -> Vec<ComponentTypeId> {
        vec![]
    }

    fn before_execution(&'a mut self) -> Self::Output {
        self
    }
}

impl<'a, V> SystemDataOutput<'a> for &'a QueryAccessor<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync + 'a,
{
    type SystemData = QueryAccessor<V>;
}

impl<V> MacroData for &'static QueryAccessor<V>
where
    V: for<'v> View<'v> + DefaultFilter,
    <V as DefaultFilter>::Filter: Send + Sync,
{
    type SystemData = QueryAccessor<V>;
}
