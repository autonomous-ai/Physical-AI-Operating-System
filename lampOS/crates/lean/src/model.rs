use crate::{
    Angles, Error, Geometry, JOINT_COUNT, Measurement, NUMERICAL_RESERVE_M, footprint::Footprint,
    invalid,
};
use glam::{DQuat, DVec3};
use serde::Serialize;
use std::f64::consts::PI;

#[derive(Clone, Copy, Debug, Serialize)]
pub struct PoseCheck {
    pub centre_of_mass_m: [f64; 3],
    /// True signed Euclidean distance to the inset polygon: positive inside,
    /// negative outside, zero on its boundary. Includes the configured margin.
    pub clearance_m: f64,
    pub measurement: Measurement,
}

impl PoseCheck {
    /// Static arithmetic only. Stand-in geometry can satisfy this test, but
    /// must never qualify actuation; see Model::has_measurements().
    pub fn within_margin(self) -> bool {
        self.clearance_m > NUMERICAL_RESERVE_M
    }
}

#[derive(Clone, Copy, Debug)]
struct Part {
    index: usize,
    origin: DVec3,
    fixed: DQuat,
    axis: DVec3,
    zero: f64,
    mass: f64,
    com: DVec3,
}

/// Validated geometry with no mutable calibration and no hardware dependency.
#[derive(Debug)]
pub struct Model {
    footprint: Footprint,
    parts: [Part; JOINT_COUNT],
    base_moment: DVec3,
    total_mass: f64,
    measurement: Measurement,
    margin: f64,
    /// Per input joint: a global COM displacement bound in metres/radian.
    pub(crate) radii: [f64; JOINT_COUNT],
}

fn position(value: [f64; 3], name: &str) -> Result<DVec3, Error> {
    if value.iter().any(|v| !v.is_finite() || v.abs() > 2.0) {
        return Err(invalid(format!("{name} must be finite metres within +/-2")));
    }
    Ok(DVec3::from_array(value))
}

fn mass(value: f64) -> Result<f64, Error> {
    if !value.is_finite() || !(0.0..=100.0).contains(&value) {
        return Err(invalid("each mass must be finite kilograms within 0..100"));
    }
    Ok(value)
}

impl Model {
    pub fn from_json(text: &str) -> Result<Self, Error> {
        Self::new(Geometry::from_json(text)?)
    }

    pub fn new(geometry: Geometry) -> Result<Self, Error> {
        if geometry.schema_version != 1 {
            return Err(invalid("geometry schema_version must be 1"));
        }
        if geometry.label.trim().is_empty() || geometry.label.len() > 256 {
            return Err(invalid("geometry needs a provenance label of 1..256 bytes"));
        }
        let footprint = Footprint::new(&geometry.footprint_m, geometry.margin_m)?;
        let base_mass = mass(geometry.base.mass_kg)?;
        if base_mass <= 0.0 {
            return Err(invalid("the fixed base must have positive mass"));
        }
        let base_com = position(geometry.base.com_m, "base COM")?;
        if base_com.z < 0.0 {
            return Err(invalid("the base COM cannot lie below its support plane"));
        }
        let mut seen = [false; JOINT_COUNT];
        let mut parts = Vec::with_capacity(JOINT_COUNT);
        for link in &geometry.links {
            let index = link.joint.index();
            if std::mem::replace(&mut seen[index], true) {
                return Err(invalid(
                    "every joint must occur exactly once in the physical chain",
                ));
            }
            let origin = position(link.origin_m, "joint origin")?;
            let com = position(link.com_m, "part COM")?;
            let axis = DVec3::from_array(link.axis);
            if !axis.is_finite() || (axis.length() - 1.0).abs() > 1e-6 {
                return Err(invalid("joint axis must be a finite unit vector"));
            }
            if link
                .rpy_rad
                .iter()
                .chain([&link.angle_zero_rad])
                .any(|q| !q.is_finite() || !(-PI..=PI).contains(q))
            {
                return Err(invalid(
                    "origin rotations and zero angles must be radians within -pi..pi",
                ));
            }
            let [roll, pitch, yaw] = link.rpy_rad;
            let fixed = DQuat::from_rotation_z(yaw)
                * DQuat::from_rotation_y(pitch)
                * DQuat::from_rotation_x(roll);
            parts.push(Part {
                index,
                origin,
                fixed,
                axis: axis.normalize(),
                zero: link.angle_zero_rad,
                mass: mass(link.mass_kg)?,
                com,
            });
        }
        let parts: [Part; JOINT_COUNT] = parts
            .try_into()
            .map_err(|_| invalid("five physical links required"))?;
        let total_mass = base_mass + parts.iter().map(|part| part.mass).sum::<f64>();
        if total_mass < 0.001 {
            return Err(invalid("total mass must be at least 0.001 kg"));
        }
        let mut radii = [0.0; JOINT_COUNT];
        for (j, joint) in parts.iter().enumerate() {
            let mut reach = 0.0;
            for (k, downstream) in parts.iter().enumerate().skip(j) {
                if k != j {
                    reach += downstream.origin.length();
                }
                radii[joint.index] +=
                    downstream.mass * (reach + downstream.com.length()) / total_mass;
            }
        }
        Ok(Self {
            footprint,
            parts,
            base_moment: base_com * base_mass,
            total_mass,
            measurement: geometry.measurement,
            margin: geometry.margin_m,
            radii,
        })
    }

    pub fn has_measurements(&self) -> bool {
        self.measurement == Measurement::Measured
    }

    pub fn total_mass_kg(&self) -> f64 {
        self.total_mass
    }

    pub fn margin_m(&self) -> f64 {
        self.margin
    }

    pub fn inset_footprint_m(&self) -> &[[f64; 2]] {
        self.footprint.vertices()
    }

    pub fn check_pose(&self, pose: Angles) -> PoseCheck {
        let angles = pose.radians();
        let mut rotation = DQuat::IDENTITY;
        let mut origin = DVec3::ZERO;
        let mut moment = self.base_moment;
        for part in &self.parts {
            origin += rotation * part.origin;
            rotation = rotation
                * part.fixed
                * DQuat::from_axis_angle(part.axis, angles[part.index] - part.zero);
            moment += (origin + rotation * part.com) * part.mass;
        }
        let centre = moment / self.total_mass;
        PoseCheck {
            centre_of_mass_m: centre.to_array(),
            clearance_m: self.footprint.clearance([centre.x, centre.y]),
            measurement: self.measurement,
        }
    }
}
