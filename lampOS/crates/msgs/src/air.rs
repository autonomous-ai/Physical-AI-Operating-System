//! The air around the lamp (`lamp-air`, `docs/air.md`): what the SEN63C
//! measures, and how the air would feel to a person at the desk.
use crate::Invalid;
use lamp_bus::{Declaration, Kind};
use serde::{Deserialize, Serialize};
use std::time::Duration;

/// How stuffy the air is, from its carbon dioxide. People begin to feel a
/// room as stuffy, and to tire, from about 1,000 to 1,400 ppm.
#[derive(Clone, Copy, Debug, PartialEq, Eq, Serialize, Deserialize)]
#[serde(rename_all = "snake_case")]
pub enum AirFeel {
    /// Under 800 ppm.
    Fresh,
    /// 800 to under 1,200 ppm.
    Fine,
    /// 1,200 to under 2,000 ppm: a window would help.
    Stuffy,
    /// 2,000 ppm or more.
    VeryStuffy,
    /// The sensor gave no carbon dioxide yet.
    Unknown,
}

impl AirFeel {
    pub fn of_co2(ppm: Option<u16>) -> Self {
        match ppm {
            None => Self::Unknown,
            Some(p) if p < 800 => Self::Fresh,
            Some(p) if p < 1_200 => Self::Fine,
            Some(p) if p < 2_000 => Self::Stuffy,
            Some(_) => Self::VeryStuffy,
        }
    }
}

/// What the air sensor read last (`air/state`). Each value is absent while
/// the sensor says it does not know it yet (just after it starts).
#[derive(Clone, Copy, Debug, PartialEq, Serialize, Deserialize)]
pub struct AirState {
    /// Particles under 1, 2.5, 4 and 10 micrometres, in micrograms a cubic
    /// metre.
    pub pm1_0: Option<f32>,
    pub pm2_5: Option<f32>,
    pub pm4_0: Option<f32>,
    pub pm10: Option<f32>,
    /// Relative humidity in percent, and temperature in degrees Celsius,
    /// at the sensor: inside the lamp's base, warmed by the lamp itself, so
    /// not the room's.
    pub humidity: Option<f32>,
    pub temperature: Option<f32>,
    /// Carbon dioxide in parts per million.
    pub co2_ppm: Option<u16>,
    /// How the air would feel, from the carbon dioxide.
    pub feel: AirFeel,
}

impl AirState {
    pub const TYPE: &str = "lamp_msgs/AirState";
    pub const VERSION: u32 = 1;
    pub const TOPIC: &str = "air/state";

    /// The newest reading replaces the one before, every five seconds.
    pub fn declaration() -> Declaration {
        Declaration::new(Self::TYPE, Self::VERSION, Kind::Latest).every(Duration::from_secs(5))
    }

    pub fn check(&self) -> Result<(), Invalid> {
        for (name, value, high) in [
            ("pm1_0", self.pm1_0, 1_000.0),
            ("pm2_5", self.pm2_5, 1_000.0),
            ("pm4_0", self.pm4_0, 1_000.0),
            ("pm10", self.pm10, 1_000.0),
            ("humidity", self.humidity, 100.0),
        ] {
            if let Some(v) = value
                && !(v.is_finite() && (0.0..=high).contains(&v))
            {
                return Err(Invalid(format!("{name} is {v}: from 0 to {high}")));
            }
        }
        if let Some(t) = self.temperature
            && !(t.is_finite() && (-40.0..=125.0).contains(&t))
        {
            return Err(Invalid(format!("temperature is {t}: from -40 to 125")));
        }
        if self.feel != AirFeel::of_co2(self.co2_ppm) {
            return Err(Invalid("feel does not follow co2_ppm".into()));
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_air_is_its_readings_and_how_it_feels() {
        let air = AirState {
            pm1_0: Some(88.8),
            pm2_5: Some(92.5),
            pm4_0: Some(92.5),
            pm10: Some(92.5),
            humidity: Some(31.7),
            temperature: Some(33.0),
            co2_ppm: Some(1_931),
            feel: AirFeel::Stuffy,
        };
        let text = serde_json::to_string(&air).unwrap();
        assert!(text.contains("\"feel\":\"stuffy\""), "{text}");
        assert_eq!(serde_json::from_str::<AirState>(&text).unwrap(), air);
        assert_eq!(air.check(), Ok(()));
        let wrong = AirState {
            feel: AirFeel::Fresh,
            ..air
        };
        assert!(wrong.check().is_err());
        let odd = AirState {
            humidity: Some(140.0),
            ..air
        };
        assert!(odd.check().unwrap_err().0.contains("humidity"));
        let declared = AirState::declaration();
        assert_eq!(
            (declared.kind, declared.period()),
            (Kind::Latest, Some(Duration::from_secs(5)))
        );
    }

    #[test]
    fn stuffiness_follows_carbon_dioxide() {
        assert_eq!(AirFeel::of_co2(None), AirFeel::Unknown);
        assert_eq!(AirFeel::of_co2(Some(450)), AirFeel::Fresh);
        assert_eq!(AirFeel::of_co2(Some(800)), AirFeel::Fine);
        assert_eq!(AirFeel::of_co2(Some(1_200)), AirFeel::Stuffy);
        assert_eq!(AirFeel::of_co2(Some(2_000)), AirFeel::VeryStuffy);
    }
}
