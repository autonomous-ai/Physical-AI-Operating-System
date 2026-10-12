//! Bounded admission into explicitly provisioned collision-free pose regions.
//!
//! Every coordinate is dimensionless `RANGE_M100_100`, in `Joint::ALL` order,
//! under one exact calibration. These cells are declarations by a provisioner,
//! not inferred geometry, measured clearance, or built-in Lamp limits. The caller
//! must bind this policy to its exact calibration and qualification record, and
//! independently check that record, placement, authority and readback freshness.
//!
//! A sweep must fit, with its tracking allowance, wholly inside one cell. This
//! covers unequal joint progress within the Cartesian hull; checking endpoints
//! separately or joining cells would not establish that property. It does not
//! cover tracking outside the allowance, unobserved motion, or an invalid region.
//! Construction and checks use fixed storage, no allocation, locks or I/O.

use crate::{JOINT_COUNT, Joint, units::NormalizedPosition};
use std::fmt;

pub const MAX_SAFE_CELLS: usize = 16;

/// One provisioner-declared region in the caller's exact calibration coordinates.
/// Validation establishes only finite, nonempty ranges, not physical clearance.
#[derive(Clone, Copy, Debug, PartialEq)]
pub struct SafeCell {
    minimum: [NormalizedPosition; JOINT_COUNT],
    maximum: [NormalizedPosition; JOINT_COUNT],
}

impl SafeCell {
    /// All five ranges must have positive width and lie within -100..=100.
    /// Bounds are inclusive when checking a pose or its complete swept hull.
    pub fn new(
        minimum: [f64; JOINT_COUNT],
        maximum: [f64; JOINT_COUNT],
    ) -> Result<Self, ClearanceError> {
        for joint in Joint::ALL {
            let i = joint.index();
            if !minimum[i].is_finite() || !maximum[i].is_finite() {
                return Err(ClearanceError::NonFiniteBounds { joint });
            }
            if !(-100.0..=100.0).contains(&minimum[i]) || !(-100.0..=100.0).contains(&maximum[i]) {
                return Err(ClearanceError::OutsideCoordinateDomain { joint });
            }
            if minimum[i] >= maximum[i] {
                return Err(ClearanceError::EmptyOrInvertedRange { joint });
            }
        }
        Ok(Self {
            minimum: minimum.map(|v| NormalizedPosition::new(v).expect("validated cell bound")),
            maximum: maximum.map(|v| NormalizedPosition::new(v).expect("validated cell bound")),
        })
    }

    pub const fn minimum(&self) -> &[NormalizedPosition; JOINT_COUNT] {
        &self.minimum
    }

    pub const fn maximum(&self) -> &[NormalizedPosition; JOINT_COUNT] {
        &self.maximum
    }
}

/// At most sixteen independent qualified regions; there is no default policy.
///
/// The caller owns calibration/qualification identity and its validity period.
/// A returned cell index is only a geometric membership result in that declared
/// coordinate system. It is neither motion authority nor a physical certificate.
#[derive(Clone, Debug, PartialEq)]
pub struct ClearancePolicy {
    cells: [SafeCell; MAX_SAFE_CELLS],
    length: usize,
}

impl ClearancePolicy {
    pub fn new(cells: &[SafeCell]) -> Result<Self, ClearanceError> {
        if cells.is_empty() {
            return Err(ClearanceError::NoCells);
        }
        if cells.len() > MAX_SAFE_CELLS {
            return Err(ClearanceError::TooManyCells { count: cells.len() });
        }
        let mut storage = [cells[0]; MAX_SAFE_CELLS];
        storage[..cells.len()].copy_from_slice(cells);
        Ok(Self {
            cells: storage,
            length: cells.len(),
        })
    }

    pub fn cells(&self) -> &[SafeCell] {
        &self.cells[..self.length]
    }

    /// Check a measured/commanded pose plus per-joint tracking uncertainty.
    pub fn check_pose(
        &self,
        pose: [NormalizedPosition; JOINT_COUNT],
        tracking_allowance: [f64; JOINT_COUNT],
    ) -> Result<usize, ClearanceError> {
        self.check_swept(pose, pose, tracking_allowance)
    }

    /// Require the complete Cartesian hull, expanded on both sides by each
    /// nonnegative allowance, inside ONE cell. At most 16 * 5 axes are checked.
    /// No endpoint clipping, tolerance, interpolation sampling or cell stitching.
    /// Returns the first containing cell's index in the original input order.
    pub fn check_swept(
        &self,
        start: [NormalizedPosition; JOINT_COUNT],
        end: [NormalizedPosition; JOINT_COUNT],
        tracking_allowance: [f64; JOINT_COUNT],
    ) -> Result<usize, ClearanceError> {
        for joint in Joint::ALL {
            let allowance = tracking_allowance[joint.index()];
            if !allowance.is_finite() || allowance < 0.0 {
                return Err(ClearanceError::InvalidTrackingAllowance { joint });
            }
        }
        self.cells()
            .iter()
            .position(|cell| {
                Joint::ALL.into_iter().all(|joint| {
                    let i = joint.index();
                    let low = start[i].get().min(end[i].get());
                    let high = start[i].get().max(end[i].get());
                    margin_fits(low, cell.minimum[i].get(), tracking_allowance[i])
                        && margin_fits(cell.maximum[i].get(), high, tracking_allowance[i])
                })
            })
            .ok_or(ClearanceError::NoQualifiedCell)
    }
}

/// Test allowance <= upper-lower without rounding an expanded hull inward.
/// Bounds are finite in [-100,100], so subtraction cannot overflow. TwoDiff's
/// residual resolves equality when the rounded gap slightly overstates the real
/// gap between the supplied floats; no epsilon silently enlarges a safe cell.
fn margin_fits(upper: f64, lower: f64, allowance: f64) -> bool {
    let gap = upper - lower;
    if allowance != gap {
        return allowance < gap;
    }
    let lower_virtual = upper - gap;
    let upper_virtual = gap + lower_virtual;
    let residual = (upper - upper_virtual) + (lower_virtual - lower);
    residual >= 0.0
}

#[derive(Clone, Copy, Debug, Eq, PartialEq)]
pub enum ClearanceError {
    NoCells,
    TooManyCells { count: usize },
    NonFiniteBounds { joint: Joint },
    OutsideCoordinateDomain { joint: Joint },
    EmptyOrInvertedRange { joint: Joint },
    InvalidTrackingAllowance { joint: Joint },
    NoQualifiedCell,
}

impl fmt::Display for ClearanceError {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            Self::NoCells => f.write_str("clearance requires at least one qualified cell"),
            Self::TooManyCells { count } => {
                write!(
                    f,
                    "clearance has {count} cells; maximum is {MAX_SAFE_CELLS}"
                )
            }
            Self::NonFiniteBounds { joint } => write!(f, "{joint} clearance bounds must be finite"),
            Self::OutsideCoordinateDomain { joint } => {
                write!(f, "{joint} clearance bounds exceed normalized -100..=100")
            }
            Self::EmptyOrInvertedRange { joint } => {
                write!(f, "{joint} clearance range must have positive width")
            }
            Self::InvalidTrackingAllowance { joint } => {
                write!(
                    f,
                    "{joint} tracking allowance must be finite and nonnegative"
                )
            }
            Self::NoQualifiedCell => {
                f.write_str("no single qualified cell contains the complete hull and allowance")
            }
        }
    }
}

impl std::error::Error for ClearanceError {}
