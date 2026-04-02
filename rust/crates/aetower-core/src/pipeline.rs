use aetower_model::{EntitySnapshot, FrontmostAppState, HostSnapshot};

use crate::{attribution, collector::RawProcessSample, friction, identity, identity::IdentityMap};

pub struct EntityPipelineOutput {
    pub identity: IdentityMap,
    pub entities: Vec<EntitySnapshot>,
}

pub fn run_entity_pipeline(
    processes: &[RawProcessSample],
    host: &HostSnapshot,
    frontmost: Option<&FrontmostAppState>,
) -> EntityPipelineOutput {
    let identity = identity::resolve(processes);
    let mut entities = attribution::build_entities(processes, &identity, frontmost);
    friction::apply(host, &mut entities);
    EntityPipelineOutput { identity, entities }
}
