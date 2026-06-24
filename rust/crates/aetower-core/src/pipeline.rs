use std::time::Instant;

use aetower_model::{EntitySnapshot, FrontmostAppState, HostSnapshot};

use crate::{attribution, collector::RawProcessSample, friction, identity};

pub struct EntityPipelineTimings {
    pub identity_millis: f64,
    pub attribution_millis: f64,
    pub friction_millis: f64,
}

pub struct EntityPipelineOutput {
    pub entities: Vec<EntitySnapshot>,
    pub timings: EntityPipelineTimings,
}

pub fn run_entity_pipeline(
    processes: &[RawProcessSample],
    host: &HostSnapshot,
    frontmost: Option<&FrontmostAppState>,
) -> EntityPipelineOutput {
    let identity_started_at = Instant::now();
    let identity = identity::resolve(processes);
    let identity_millis = elapsed_millis(identity_started_at);

    let attribution_started_at = Instant::now();
    let mut entities = attribution::build_entities(processes, &identity, frontmost);
    let attribution_millis = elapsed_millis(attribution_started_at);

    let friction_started_at = Instant::now();
    friction::apply(host, &mut entities);
    let friction_millis = elapsed_millis(friction_started_at);

    EntityPipelineOutput {
        entities,
        timings: EntityPipelineTimings {
            identity_millis,
            attribution_millis,
            friction_millis,
        },
    }
}

fn elapsed_millis(started_at: Instant) -> f64 {
    started_at.elapsed().as_secs_f64() * 1000.0
}
