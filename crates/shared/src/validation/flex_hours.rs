use crate::{
    dto::{
        flex_hours::{DecideFlexRequest, RequestFlexRequest},
        policy::PolicyDto,
    },
    errors::SharedError,
    validation::{
        common::{self, DESCRIPTION_MAX},
        policy,
    },
};

/// Validates a flex-hours request against the policy limits fetched client-side,
/// mirroring `domain::model::FlexHours::validate_day`: block count, ordering,
/// non-overlap, envelope containment, core-hour coverage and the daily band.
///
/// # Errors
/// Returns [`SharedError::Validation`] describing the first rule violated.
pub fn validate_flex(req: &RequestFlexRequest, policy: &PolicyDto) -> Result<(), SharedError> {
    common::iso_date("Work date", &req.work_date)?;
    let max = usize::from(policy.flex_max_segments);
    if req.segments.is_empty() {
        return Err(SharedError::Validation(
            "Add at least one work block".into(),
        ));
    }
    if req.segments.len() > max {
        return Err(SharedError::Validation(format!(
            "At most {max} work blocks are allowed"
        )));
    }

    let earliest = policy::minutes(&policy.flex_earliest_start, "Flex earliest start")?;
    let latest = policy::minutes(&policy.flex_latest_end, "Flex latest end")?;
    let core_start = policy::minutes(&policy.flex_core_start, "Flex core start")?;
    let core_end = policy::minutes(&policy.flex_core_end, "Flex core end")?;

    let mut blocks: Vec<(u16, u16)> = Vec::with_capacity(req.segments.len());
    for seg in &req.segments {
        let start = policy::minutes(&seg.start, "Block start")?;
        let end = policy::minutes(&seg.end, "Block end")?;
        if end <= start {
            return Err(SharedError::Validation(
                "Each block must end after it starts".into(),
            ));
        }
        if start < earliest || end > latest {
            return Err(SharedError::Validation(
                "Blocks must fall within the allowed start/end window".into(),
            ));
        }
        blocks.push((start, end));
    }

    for pair in blocks.windows(2) {
        if pair[1].0 < pair[0].1 {
            return Err(SharedError::Validation(
                "Blocks must be ordered and must not overlap".into(),
            ));
        }
    }

    let mut covered = core_start;
    for (start, end) in &blocks {
        if *start <= covered {
            if *end > covered {
                covered = *end;
            }
        } else {
            break;
        }
        if covered >= core_end {
            break;
        }
    }
    if covered < core_end {
        return Err(SharedError::Validation(
            "Blocks must cover the required core hours".into(),
        ));
    }

    let total: u16 = blocks.iter().map(|(s, e)| e - s).sum();
    let hours = f64::from(total) / 60.0;
    common::in_range(
        "Daily total",
        hours,
        policy.flex_daily_min,
        policy.flex_daily_max,
    )?;
    Ok(())
}

/// Validates a flex-hours decision: a bounded note.
///
/// # Errors
/// Returns [`SharedError::Validation`] when the note is too long.
pub fn validate_decide_flex(req: &DecideFlexRequest) -> Result<(), SharedError> {
    common::max_len("Note", &req.note, DESCRIPTION_MAX)?;
    Ok(())
}
