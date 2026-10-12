//! All regions below are SYNTHETIC logical fixtures, not Lamp qualification.

use lamp_motor::{
    JOINT_COUNT, Joint,
    clearance::{ClearanceError, ClearancePolicy, MAX_SAFE_CELLS, SafeCell},
    units::NormalizedPosition,
};

fn pose(values: [f64; JOINT_COUNT]) -> [NormalizedPosition; JOINT_COUNT] {
    values.map(|value| NormalizedPosition::new(value).unwrap())
}

fn box_cell(low: f64, high: f64) -> SafeCell {
    SafeCell::new([low; JOINT_COUNT], [high; JOINT_COUNT]).unwrap()
}

#[test]
fn separately_allowed_endpoints_do_not_authorize_a_disconnected_sweep() {
    let policy = ClearancePolicy::new(&[box_cell(-10.0, -1.0), box_cell(1.0, 10.0)]).unwrap();
    let start = pose([-5.0; JOINT_COUNT]);
    let end = pose([5.0; JOINT_COUNT]);
    assert_eq!(policy.check_pose(start, [0.0; JOINT_COUNT]), Ok(0));
    assert_eq!(policy.check_pose(end, [0.0; JOINT_COUNT]), Ok(1));
    assert_eq!(
        policy.check_pose(pose([0.0; JOINT_COUNT]), [0.0; JOINT_COUNT]),
        Err(ClearanceError::NoQualifiedCell)
    );
    assert_eq!(
        policy.check_swept(start, end, [0.0; JOINT_COUNT]),
        Err(ClearanceError::NoQualifiedCell)
    );
}

#[test]
fn overlapping_cells_cannot_be_stitched_into_a_sweep_certificate() {
    let policy = ClearancePolicy::new(&[box_cell(-10.0, 1.0), box_cell(-1.0, 10.0)]).unwrap();
    // The center and both endpoints are individually allowed, but the policy
    // deliberately requires one qualified region for the complete operation.
    for value in [-5.0, 0.0, 5.0] {
        assert!(
            policy
                .check_pose(pose([value; JOINT_COUNT]), [0.0; JOINT_COUNT])
                .is_ok()
        );
    }
    assert_eq!(
        policy.check_swept(
            pose([-5.0; JOINT_COUNT]),
            pose([5.0; JOINT_COUNT]),
            [0.0; JOINT_COUNT],
        ),
        Err(ClearanceError::NoQualifiedCell)
    );
}

#[test]
fn synthetic_head_base_collision_shape_requires_the_whole_body_hull() {
    // These invented normalized regions encode a logical support/head coupling.
    // They reproduce no actual Lamp geometry, encoder range or clearance.
    let mut lower = [-10.0; JOINT_COUNT];
    let mut upper = [10.0; JOINT_COUNT];
    lower[Joint::BasePitch.index()] = -4.0;
    upper[Joint::BasePitch.index()] = -1.0;
    lower[Joint::WristPitch.index()] = -4.0;
    upper[Joint::WristPitch.index()] = -1.0;
    let folded = SafeCell::new(lower, upper).unwrap();
    lower[Joint::BasePitch.index()] = 1.0;
    upper[Joint::BasePitch.index()] = 4.0;
    lower[Joint::WristPitch.index()] = 1.0;
    upper[Joint::WristPitch.index()] = 4.0;
    let raised = SafeCell::new(lower, upper).unwrap();
    let policy = ClearancePolicy::new(&[folded, raised]).unwrap();
    let start = pose([0.0, -2.0, 0.0, 0.0, -2.0]);
    let end = pose([0.0, 2.0, 0.0, 0.0, 2.0]);
    assert_eq!(policy.check_pose(start, [0.0; JOINT_COUNT]), Ok(0));
    assert_eq!(policy.check_pose(end, [0.0; JOINT_COUNT]), Ok(1));
    // Each axis remains in the independent [-10,10] register-like limit.
    // Head progress before its support moves nevertheless leaves both regions.
    let head_first = pose([0.0, -2.0, 0.0, 0.0, 2.0]);
    assert_eq!(
        policy.check_pose(head_first, [0.0; JOINT_COUNT]),
        Err(ClearanceError::NoQualifiedCell)
    );
    assert_eq!(
        policy.check_swept(start, end, [0.0; JOINT_COUNT]),
        Err(ClearanceError::NoQualifiedCell)
    );
    // A local head change inside the qualified support-dependent region fits.
    assert_eq!(
        policy.check_swept(
            start,
            pose([0.0, -2.0, 0.0, 0.0, -3.0]),
            [0.25; JOINT_COUNT]
        ),
        Ok(0)
    );
}

#[test]
fn entire_unequal_joint_progress_hull_is_covered_by_one_cell() {
    let policy = ClearancePolicy::new(&[box_cell(-4.0, 4.0)]).unwrap();
    let start = pose([-2.0, 2.0, -1.0, 3.0, -3.0]);
    let end = pose([2.0, -2.0, 1.0, -3.0, 3.0]);
    assert_eq!(policy.check_swept(start, end, [1.0; JOINT_COUNT]), Ok(0));
    // Every independently advanced combination is in the same region, including
    // combinations not visited by a synchronized straight-line interpolation.
    for mask in 0..(1 << JOINT_COUNT) {
        let corner = std::array::from_fn(|i| {
            if mask & (1 << i) == 0 {
                start[i]
            } else {
                end[i]
            }
        });
        assert_eq!(policy.check_pose(corner, [1.0; JOINT_COUNT]), Ok(0));
    }
}

#[test]
fn exact_boundaries_are_inclusive_but_positive_uncertainty_must_fit() {
    let policy = ClearancePolicy::new(&[box_cell(-1.0, 1.0)]).unwrap();
    assert_eq!(
        policy.check_swept(
            pose([-1.0; JOINT_COUNT]),
            pose([1.0; JOINT_COUNT]),
            [0.0; JOINT_COUNT]
        ),
        Ok(0)
    );
    assert_eq!(
        policy.check_pose(pose([0.0; JOINT_COUNT]), [1.0; JOINT_COUNT]),
        Ok(0)
    );
    assert_eq!(
        policy.check_pose(pose([0.0; JOINT_COUNT]), [1.0_f64.next_up(); JOINT_COUNT]),
        Err(ClearanceError::NoQualifiedCell)
    );
    // Even uncertainty too small to change a rounded +/-1.0 must not vanish.
    for edge in [-1.0, 1.0] {
        let mut allowance = [0.0; JOINT_COUNT];
        allowance[Joint::WristPitch.index()] = f64::from_bits(1);
        assert_eq!(
            policy.check_pose(pose([edge; JOINT_COUNT]), allowance),
            Err(ClearanceError::NoQualifiedCell)
        );
    }
}

#[test]
fn rounded_gap_cannot_silently_enlarge_the_qualified_region() {
    let mut minimum = [-2.0; JOINT_COUNT];
    minimum[0] = -1.0;
    let policy =
        ClearancePolicy::new(&[SafeCell::new(minimum, [2.0; JOINT_COUNT]).unwrap()]).unwrap();
    let mut point = [0.0; JOINT_COUNT];
    point[0] = -f64::EPSILON / 4.0;
    let mut allowance = [0.0; JOINT_COUNT];
    allowance[0] = 1.0;
    // Floating subtraction rounds (point - minimum) to 1.0, although the
    // mathematical gap between these supplied float values is slightly less.
    assert_eq!(point[0] - minimum[0], 1.0);
    assert_eq!(
        policy.check_pose(pose(point), allowance),
        Err(ClearanceError::NoQualifiedCell)
    );
}

#[test]
fn tracking_allowance_applies_to_support_joints_too() {
    let policy = ClearancePolicy::new(&[box_cell(-1.0, 1.0)]).unwrap();
    let mut allowance = [0.1; JOINT_COUNT];
    allowance[Joint::BasePitch.index()] = 0.3;
    assert_eq!(
        policy.check_swept(
            pose([0.0, 0.8, 0.0, 0.0, -0.5]),
            pose([0.0, 0.8, 0.0, 0.0, 0.5]),
            allowance
        ),
        Err(ClearanceError::NoQualifiedCell)
    );
    allowance[Joint::BasePitch.index()] = 0.125;
    assert_eq!(
        policy.check_swept(
            pose([0.0, 0.8, 0.0, 0.0, -0.5]),
            pose([0.0, 0.8, 0.0, 0.0, 0.5]),
            allowance
        ),
        Ok(0)
    );
}

#[test]
fn empty_and_excess_cell_sets_are_rejected_without_truncation() {
    assert_eq!(ClearancePolicy::new(&[]), Err(ClearanceError::NoCells));
    let cells = [box_cell(-1.0, 1.0); MAX_SAFE_CELLS + 1];
    assert_eq!(
        ClearancePolicy::new(&cells),
        Err(ClearanceError::TooManyCells {
            count: MAX_SAFE_CELLS + 1
        })
    );
    assert_eq!(
        ClearancePolicy::new(&cells[..MAX_SAFE_CELLS])
            .unwrap()
            .cells()
            .len(),
        MAX_SAFE_CELLS
    );
}

#[test]
fn the_last_cell_is_checked_and_original_order_is_preserved() {
    let mut cells = [box_cell(-10.0, -5.0); MAX_SAFE_CELLS];
    cells[MAX_SAFE_CELLS - 1] = box_cell(5.0, 10.0);
    let policy = ClearancePolicy::new(&cells).unwrap();
    assert_eq!(
        policy.check_pose(pose([7.0; JOINT_COUNT]), [0.0; JOINT_COUNT]),
        Ok(MAX_SAFE_CELLS - 1)
    );
    assert_eq!(
        policy.check_pose(pose([-7.0; JOINT_COUNT]), [0.0; JOINT_COUNT]),
        Ok(0)
    );
}

#[test]
fn invalid_ranges_are_rejected_in_every_joint() {
    for joint in Joint::ALL {
        let i = joint.index();
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
            let mut minimum = [-1.0; JOINT_COUNT];
            minimum[i] = bad;
            assert_eq!(
                SafeCell::new(minimum, [1.0; JOINT_COUNT]),
                Err(ClearanceError::NonFiniteBounds { joint })
            );
            let mut maximum = [1.0; JOINT_COUNT];
            maximum[i] = bad;
            assert_eq!(
                SafeCell::new([-1.0; JOINT_COUNT], maximum),
                Err(ClearanceError::NonFiniteBounds { joint })
            );
        }
        for low in [1.0, 2.0] {
            let mut minimum = [-1.0; JOINT_COUNT];
            minimum[i] = low;
            assert_eq!(
                SafeCell::new(minimum, [1.0; JOINT_COUNT]),
                Err(ClearanceError::EmptyOrInvertedRange { joint })
            );
        }
        let mut minimum = [-1.0; JOINT_COUNT];
        minimum[i] = -100.0_f64.next_up();
        assert_eq!(
            SafeCell::new(minimum, [1.0; JOINT_COUNT]),
            Err(ClearanceError::OutsideCoordinateDomain { joint })
        );
        let mut maximum = [1.0; JOINT_COUNT];
        maximum[i] = 100.0_f64.next_up();
        assert_eq!(
            SafeCell::new([-1.0; JOINT_COUNT], maximum),
            Err(ClearanceError::OutsideCoordinateDomain { joint })
        );
    }
}

#[test]
fn invalid_allowances_cannot_hide_behind_a_matching_first_cell() {
    let policy = ClearancePolicy::new(&[box_cell(-100.0, 100.0)]).unwrap();
    for joint in Joint::ALL {
        for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY, -0.001] {
            let mut allowance = [0.0; JOINT_COUNT];
            allowance[joint.index()] = bad;
            assert_eq!(
                policy.check_pose(pose([0.0; JOINT_COUNT]), allowance),
                Err(ClearanceError::InvalidTrackingAllowance { joint })
            );
        }
    }
    assert_eq!(
        policy.check_pose(pose([0.0; JOINT_COUNT]), [f64::MAX; JOINT_COUNT]),
        Err(ClearanceError::NoQualifiedCell)
    );
    assert_eq!(
        policy.check_pose(pose([0.0; JOINT_COUNT]), [-0.0; JOINT_COUNT]),
        Ok(0)
    );
}
