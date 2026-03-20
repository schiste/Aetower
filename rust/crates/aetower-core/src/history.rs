use std::collections::{BTreeMap, VecDeque};

use aetower_model::{EntitySnapshot, TimelineEvent, TimelineSeverity};

pub struct History {
    previous_scores: BTreeMap<String, f32>,
    timeline: VecDeque<TimelineEvent>,
}

impl History {
    pub fn new() -> Self {
        Self {
            previous_scores: BTreeMap::new(),
            timeline: VecDeque::with_capacity(256),
        }
    }

    pub fn update(&mut self, captured_at_millis: u64, entities: &[EntitySnapshot]) -> Vec<TimelineEvent> {
        for entity in entities.iter().take(5) {
            let previous = self.previous_scores.get(&entity.entity_id).copied().unwrap_or_default();
            let delta = entity.friction.total_score - previous;
            if previous == 0.0 && entity.friction.total_score >= 20.0 {
                self.push_event(
                    captured_at_millis,
                    TimelineSeverity::Warning,
                    Some(entity.entity_id.clone()),
                    format!("{} became active", entity.display_name),
                    entity.friction.reasons.join(", "),
                );
            } else if delta >= 15.0 {
                self.push_event(
                    captured_at_millis,
                    TimelineSeverity::Critical,
                    Some(entity.entity_id.clone()),
                    format!("{} friction spiked", entity.display_name),
                    entity.friction.reasons.join(", "),
                );
            }
            self.previous_scores
                .insert(entity.entity_id.clone(), entity.friction.total_score);
        }

        while self.timeline.len() > 120 {
            self.timeline.pop_front();
        }

        self.timeline.iter().cloned().collect()
    }

    fn push_event(
        &mut self,
        timestamp_millis: u64,
        severity: TimelineSeverity,
        entity_id: Option<String>,
        title: String,
        detail: String,
    ) {
        self.timeline.push_back(TimelineEvent {
            id: format!(
                "{}:{}",
                entity_id.clone().unwrap_or_else(|| "system".to_owned()),
                timestamp_millis
            ),
            timestamp_millis,
            severity,
            entity_id,
            title,
            detail,
        });
    }
}
