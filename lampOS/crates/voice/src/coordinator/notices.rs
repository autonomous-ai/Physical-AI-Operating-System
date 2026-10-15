//! One local notice for a still-owned failed reply. No provider or device I/O.
use super::*;
use crate::options::NoticeOptions;
use lamp_audio::notice::{Approval, Missing, NoticeInfo, NoticeKind, Notices};
use lamp_audio::ownership::PlaybackCursor;
use std::sync::Arc;

/// Preloaded once, before any audio worker is started. The selected recording
/// is immutable and bounded by the notice loader; a turn holds only an Arc.
pub(super) struct FailureNotices {
    selected: std::result::Result<Arc<Recording>, Missing>,
    language: Option<String>,
}
pub(super) struct Recording {
    /// The provisioned notice; none for the self-test's tone, which is
    /// made here and says nothing.
    info: Option<NoticeInfo>,
    samples: Box<[i16]>,
}
impl FailureNotices {
    pub(super) fn load(options: Option<&NoticeOptions>) -> Self {
        Self::load_with_approval(options, Approval::Production)
    }
    fn load_with_approval(options: Option<&NoticeOptions>, approval: Approval) -> Self {
        let Some(options) = options else {
            return Self::none();
        };
        let loaded = Notices::load(options.directory(), approval);
        let selected = loaded
            .get(NoticeKind::TurnLost, options.language())
            .map(|notice| {
                Arc::new(Recording {
                    info: Some(notice.info.clone()),
                    samples: notice.samples().into(),
                })
            });
        Self {
            selected,
            language: Some(options.language().to_owned()),
        }
    }
    pub(super) fn none() -> Self {
        Self {
            selected: Err(Missing::NoDirectory),
            language: None,
        }
    }
    pub(super) fn capability(&self) -> Value {
        match &self.selected {
            Ok(recording) => {
                json!({"configured":true,"language":self.language,"available":recording.info})
            }
            Err(reason) => {
                json!({"configured":self.language.is_some(),"language":self.language,"missing":reason})
            }
        }
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq, serde::Serialize)]
#[serde(rename_all = "snake_case")]
pub(super) enum SpeechOrigin {
    Provider,
    FailureNotice,
    /// The self-test's tone (`crate::speaker_check`): local PCM, no answer.
    SelfTest,
}
impl SpeechOrigin {
    /// Played from a recording held here, not from the provider.
    pub(super) fn local(self) -> bool {
        self != Self::Provider
    }
    /// The trace's name for what plays.
    pub(super) fn trace_name(self) -> &'static str {
        match self {
            Self::Provider => "speaker",
            Self::FailureNotice => "failure_notice",
            Self::SelfTest => "selftest_tone",
        }
    }
}

/// One attempt, retained independently of best-effort observation storage.
/// Selection happens only after prior provider PCM has actually retired.
#[derive(Default)]
pub(super) enum NoticeDelivery {
    #[default]
    Unselected,
    Skipped(Missing),
    Selected {
        /// `FailureNotice` or `SelfTest`.
        origin: SpeechOrigin,
        recording: Arc<Recording>,
        cursor: usize,
        attempted_at_us: Option<u64>,
        /// When the speaker first accepted frames of it.
        first_accepted_at_us: Option<u64>,
        retired_at_us: Option<u64>,
        final_acceptance: Option<(u64, PlaybackCursor)>,
        gaps: u64,
    },
}
impl NoticeDelivery {
    pub(super) fn origin(&self) -> SpeechOrigin {
        match self {
            Self::Selected { origin, .. } => *origin,
            _ => SpeechOrigin::Provider,
        }
    }
    /// The self-test's tone, ready to play from its first sample.
    pub(super) fn tone(samples: Vec<i16>) -> Self {
        Self::Selected {
            origin: SpeechOrigin::SelfTest,
            recording: Arc::new(Recording {
                info: None,
                samples: samples.into_boxed_slice(),
            }),
            cursor: 0,
            attempted_at_us: None,
            first_accepted_at_us: None,
            retired_at_us: None,
            final_acceptance: None,
            gaps: 0,
        }
    }
    /// The speaker's first acceptance of frames of it, once.
    pub(super) fn first_accepted(&mut self, at_us: u64) {
        if let Self::Selected {
            first_accepted_at_us: first @ None,
            ..
        } = self
        {
            *first = Some(at_us);
        }
    }
    /// When the speaker first accepted frames of it.
    pub(super) fn first_accepted_at_us(&self) -> Option<u64> {
        match self {
            Self::Selected {
                first_accepted_at_us,
                ..
            } => *first_accepted_at_us,
            _ => None,
        }
    }
    /// Whether all of it was played: its final sample retired.
    pub(super) fn retired(&self) -> bool {
        matches!(
            self,
            Self::Selected {
                retired_at_us: Some(_),
                ..
            }
        )
    }
    pub(super) fn settled(&self) -> bool {
        !matches!(
            self,
            Self::Selected {
                retired_at_us: None,
                ..
            }
        )
    }
    pub(super) fn has_chunk(&self) -> bool {
        matches!(self, Self::Selected { recording, cursor, retired_at_us:None, .. } if *cursor < recording.samples.len())
    }
    /// Called only after the current owner passes the speech-permit check.
    /// Mark before transport. The caller propagates send failure into ordinary
    /// cancellation, retaining attempted evidence and never restarting this clip.
    pub(super) fn reserve_chunk(&mut self, at_us: u64) -> io::Result<(Vec<i16>, bool, bool)> {
        let Self::Selected {
            recording,
            cursor,
            attempted_at_us,
            retired_at_us: None,
            ..
        } = self
        else {
            return Err(io::Error::other(
                "notice dispatch without selected live recording",
            ));
        };
        if at_us == 0 || *cursor >= recording.samples.len() {
            return Err(io::Error::other(
                "notice exhausted or invalid dispatch time",
            ));
        }
        let first = attempted_at_us.is_none();
        if first {
            *attempted_at_us = Some(at_us);
        }
        let end = (*cursor + 240).min(recording.samples.len());
        let mut samples = recording.samples[*cursor..end].to_vec();
        samples.resize(240, 0);
        *cursor = end;
        Ok((samples, end == recording.samples.len(), first))
    }
    /// Bound to the real full final-chunk acceptance, not dispatch or queue time.
    pub(super) fn accepted_final(
        &mut self,
        sequence: u64,
        cursor: PlaybackCursor,
    ) -> io::Result<()> {
        let Self::Selected {
            recording,
            cursor: reserved,
            attempted_at_us: Some(_),
            retired_at_us: None,
            final_acceptance,
            ..
        } = self
        else {
            return Err(io::Error::other(
                "notice final acceptance without attempted recording",
            ));
        };
        if *reserved != recording.samples.len()
            || sequence == 0
            || cursor.epoch == 0
            || cursor.frame == 0
            || final_acceptance.is_some()
        {
            return Err(io::Error::other("invalid notice final acceptance"));
        }
        *final_acceptance = Some((sequence, cursor));
        Ok(())
    }
    pub(super) fn validate_retirement(
        &self,
        sequence: u64,
        cursor: PlaybackCursor,
    ) -> io::Result<()> {
        if !matches!(self, Self::Selected {
            final_acceptance: Some(accepted), retired_at_us: None, ..
        } if *accepted == (sequence, cursor))
        {
            return Err(io::Error::other(
                "notice retirement differs from actual final acceptance",
            ));
        }
        Ok(())
    }
    pub(super) fn retire(&mut self, at_us: u64) -> io::Result<()> {
        let Self::Selected {
            recording,
            cursor,
            attempted_at_us: Some(start),
            retired_at_us,
            ..
        } = self
        else {
            return Err(io::Error::other(
                "notice retirement without an attempted recording",
            ));
        };
        if *cursor != recording.samples.len() || retired_at_us.is_some() || at_us < *start {
            return Err(io::Error::other(
                "notice retirement before exact final delivery",
            ));
        }
        *retired_at_us = Some(at_us);
        Ok(())
    }
    pub(super) fn gap(&mut self) -> io::Result<()> {
        let Self::Selected { gaps, .. } = self else {
            return Err(io::Error::other("notice gap without recording"));
        };
        *gaps = gaps
            .checked_add(1)
            .ok_or_else(|| io::Error::other("notice gap counter exhausted"))?;
        Ok(())
    }
    pub(super) fn evidence(&self) -> Value {
        match self {
            Self::Unselected => Value::Null,
            Self::Skipped(reason) => json!({"state":"skipped","reason":reason}),
            Self::Selected {
                recording,
                cursor,
                attempted_at_us,
                retired_at_us,
                gaps,
                ..
            } => {
                let state = if retired_at_us.is_some() {
                    "retired"
                } else if attempted_at_us.is_some() {
                    "attempted"
                } else {
                    "selected"
                };
                match &recording.info {
                    Some(info) => json!({
                        "state":state,
                        "kind":info.kind,"wav_sha256":info.wav_sha256,
                        "approval":info.approval,"frames":info.frames,
                        "reserved_frames":cursor,"attempted_at_us":attempted_at_us,
                        "retired_at_us":retired_at_us,"playback_gaps":gaps,
                        "answer":false,"acoustic_delivery_verified":false}),
                    None => json!({
                        "state":state,"kind":"selftest_tone",
                        "frames":recording.samples.len(),
                        "reserved_frames":cursor,"attempted_at_us":attempted_at_us,
                        "retired_at_us":retired_at_us,"playback_gaps":gaps,
                        "answer":false,"acoustic_delivery_verified":false}),
                }
            }
        }
    }
}

/// No owner is minted here. A completed, cancelled or never-owned request
/// cannot acquire this recording. Selection does not grant a speaker permit.
pub(super) fn select_failure_notice(
    catalog: &FailureNotices,
    reply: &mut Option<Reply>,
    pending: &Option<PendingSpeaker>,
    trace: &mut SessionState,
) -> Result<()> {
    let Some(current) = reply.as_mut() else {
        return Ok(());
    };
    // An autonomous reminder that cannot be delivered stays pending silently.
    // A local failure recording cannot stand in for the intended reminder.
    if current.reminder.is_some() {
        return Ok(());
    }
    if !matches!(current.notice, NoticeDelivery::Unselected)
        || !matches!(current.cloud.outcome, Some(RequestOutcome::Failed { .. }))
        || !current.local_input_ended
        || !current.provider_idle
        || !current.pcm.is_empty()
        || current.playback.is_some()
        || current.final_chunk.is_some()
        || pending.is_some()
    {
        return Ok(());
    }
    current.notice = match &catalog.selected {
        Ok(recording) => NoticeDelivery::Selected {
            origin: SpeechOrigin::FailureNotice,
            recording: Arc::clone(recording),
            cursor: 0,
            attempted_at_us: None,
            first_accepted_at_us: None,
            retired_at_us: None,
            final_acceptance: None,
            gaps: 0,
        },
        Err(reason) => NoticeDelivery::Skipped(*reason),
    };
    record(
        trace,
        json!({"kind":"failure_notice_selected","at_us":monotonic_us(),
        "owner":current.owner,"turn":current.owner.turn(),"notice":current.notice.evidence(),
        "provider_outcome":current.cloud.outcome,"output_authority_granted":false}),
    )
}

#[cfg(test)]
impl FailureNotices {
    pub(super) fn synthetic() -> Self {
        // Test-only PCM. No manifest or production approval is created.
        let samples = vec![321_i16; 7_200].into_boxed_slice();
        Self {
            selected: Ok(Arc::new(Recording {
                info: Some(NoticeInfo {
                    kind: NoticeKind::TurnLost,
                    language: "en-US".into(),
                    transcript: "Synthetic PCM for tests; not a voice asset".into(),
                    voice: "test".into(),
                    voice_revision: "test".into(),
                    approval: Approval::Test,
                    wav_sha256: "test-only-no-wav".into(),
                    pcm_sha256: "test-only".into(),
                    frames: 7_200,
                    duration_ms: 300,
                }),
                samples,
            })),
            language: Some("en-US".into()),
        }
    }
}

#[cfg(test)]
mod loading_tests {
    use super::*;
    use sha2::{Digest, Sha256};

    struct Directory(PathBuf);
    impl Drop for Directory {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }
    fn directory() -> Directory {
        let path = std::env::temp_dir().join(format!(
            "lamp-notice-test-{}",
            boot_text(new_boot().unwrap())
        ));
        fs::create_dir(&path).unwrap();
        Directory(path)
    }
    fn test_asset(directory: &Path) -> (crate::options::NoticeOptions, PathBuf) {
        let wav = directory.join("lost.wav");
        let mut writer = hound::WavWriter::create(
            &wav,
            hound::WavSpec {
                channels: 1,
                sample_rate: 24_000,
                bits_per_sample: 16,
                sample_format: hound::SampleFormat::Int,
            },
        )
        .unwrap();
        for _ in 0..7_200 {
            writer.write_sample(321_i16).unwrap();
        }
        writer.finalize().unwrap();
        let hash = format!("{:x}", Sha256::digest(fs::read(&wav).unwrap()));
        let manifest = json!({"schema_version":1,"notices":[{"kind":"turn_lost","language":"en-US",
            "transcript":"Synthetic PCM, not a voice asset","voice":"test","voice_revision":"test",
            "approval":"test","file":"lost.wav","wav_sha256":hash,"frames":7200}]});
        fs::write(
            directory.join("manifest.json"),
            serde_json::to_vec(&manifest).unwrap(),
        )
        .unwrap();
        (
            crate::options::NoticeOptions::new(directory.into(), "en-US".into()).unwrap(),
            wav,
        )
    }
    #[test]
    fn production_never_accepts_test_pcm_or_unknown_language() {
        let directory = directory();
        let (options, _) = test_asset(&directory.0);
        let catalog = FailureNotices::load(Some(&options));
        assert_eq!(catalog.capability()["missing"], "not_approved");
        let test_only = FailureNotices::load_with_approval(Some(&options), Approval::Test);
        assert_eq!(test_only.capability()["available"]["approval"], "test");
        let other = NoticeOptions::new(directory.0.clone(), "en".into()).unwrap();
        assert_eq!(
            FailureNotices::load_with_approval(Some(&other), Approval::Test).capability()["missing"],
            "not_provisioned"
        );
    }
    #[test]
    fn preloaded_notice_is_immutable_and_missing_corrupt_assets_have_bounded_reasons() {
        let directory = directory();
        let (options, wav) = test_asset(&directory.0);
        let loaded = FailureNotices::load_with_approval(Some(&options), Approval::Test);
        fs::write(&wav, b"changed after preload").unwrap();
        let catalog = FailureNotices::load_with_approval(Some(&options), Approval::Test);
        assert_eq!(catalog.capability()["missing"], "asset_hash_mismatch");
        match &loaded.selected {
            Ok(recording) => assert_eq!(&*recording.samples, &vec![321; 7200]),
            Err(reason) => panic!("test load refused {reason:?}"),
        }
        fs::remove_file(&wav).unwrap();
        assert_eq!(
            FailureNotices::load_with_approval(Some(&options), Approval::Test).capability()["missing"],
            "asset_missing"
        );
        fs::write(directory.0.join("manifest.json"), b"{}").unwrap();
        assert_eq!(
            FailureNotices::load(Some(&options)).capability()["missing"],
            "invalid_manifest"
        );
    }
}
