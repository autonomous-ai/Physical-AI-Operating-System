use lamp_voice::{config::ProviderConfig, process::parse_boot, transport::WorkerChannels};
use std::{io, path::Path};
type Result<T> = std::result::Result<T, Box<dyn std::error::Error + Send + Sync>>;

fn main() {
    // The service and its workers: a log line never blocks them or ends
    // them (`lamp_voice::log_relay`), and their last lines are passed on.
    let relayed = matches!(std::env::args().nth(1).as_deref(), Some("serve" | "worker"))
        && lamp_voice::log_relay::install()
            .map_err(|error| eprintln!("lamp-voice: log lines go straight to the journal: {error}"))
            .is_ok();
    let result = run();
    if let Err(error) = &result {
        eprintln!("lamp-voice: {error}");
    }
    if relayed {
        lamp_voice::log_relay::finish(std::time::Duration::from_millis(500));
    }
    if result.is_err() {
        std::process::exit(1);
    }
}
fn run() -> Result<()> {
    let args = std::env::args().skip(1).collect::<Vec<_>>();
    match args.as_slice() {
        [command] if command == "schedule-consumer" => {
            lamp_voice::reminders::run_consumer()?;
        },
        // The same with one JPEG beside the note: the lamp taking a look.
        [command,path,flag,picture,note] if command=="provider-check" && flag=="--look" => {
            let config=ProviderConfig::load(Path::new(path))?.into_session()?;
            let jpeg=std::fs::read(picture)?;
            tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
                use lamp_gemini::Event;
                let mut connection=lamp_gemini::connect(config,lamp_gemini::SessionId::new(1)?).await?;
                let input=connection.input();
                let request=lamp_gemini::RequestId::new(1)?;
                input.try_written(request,note,Some(&jpeg))?;
                let asked=lamp_ipc::monotonic_us();
                let (mut said,mut samples,mut first,mut ended)=(String::new(),0usize,None,"timeout");
                let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(15);
                loop {
                    match tokio::time::timeout_at(deadline,connection.next_event()).await {
                        Ok(Some(Event::Audio{pcm,..})) => { first.get_or_insert(lamp_ipc::monotonic_us()-asked); samples+=pcm.len(); }
                        Ok(Some(Event::OutputTranscript{text,..})) => said.push_str(&text),
                        Ok(Some(Event::TurnComplete{..})) => { ended="turn_complete"; break; }
                        Ok(Some(Event::Unanswered{..})) => { ended="unanswered"; break; }
                        Ok(Some(_)) => {}
                        Ok(None) => { ended="connection_ended"; break; }
                        Err(_) => break,
                    }
                }
                println!("{}",serde_json::json!({"status":"look_tried","picture_bytes":jpeg.len(),"ended":ended,"first_audio_us":first,"audio_samples":samples,"said":said,"state":format!("{:?}",connection.state()),"microphone_opened":false,"audio_sent":false}));
                connection.shutdown();
                Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
            })?;
        },
        // A whole turn with a written note in place of speech, to see what the
        // service makes of one. Nothing is captured and nothing is played.
        // A written request to a session that was told of two functions:
        // the voice's own way of saying a thing was not for it, and one of
        // the kind the executive will offer. Does the service take the
        // declarations, which does the model call for which words, what
        // does it say after the answer, and how long does each step take?
        // Every call is answered as done. Nothing is captured or played,
        // and nothing on the lamp is changed.
        [command,path,flag,then,note] if command=="provider-check" && flag=="--tools" && then=="--written" => {
            let own=lamp_voice::tools::declared(&lamp_voice::tools::Settings{decline:true,exec:None,memory:None,clock:false,..Default::default()});
            let colour=lamp_gemini::Tool{name:"light_colour".into(),description:"Set the colour of your own light when you are asked to.".into(),parameters:Some(serde_json::json!({"type":"object","properties":{"colour":{"type":"string","enum":["white","warm","red","green","blue","purple"]}},"required":["colour"]})),silent:false};
            // LAMP_PROBE_PLAIN set: the voice's own function is declared as an ordinary one, to compare.
            let own:Vec<lamp_gemini::Tool>=own.into_iter().map(|mut tool|{tool.silent=std::env::var_os("LAMP_PROBE_PLAIN").is_none();tool}).collect();
            let config=ProviderConfig::load(Path::new(path))?.into_session()?.tools(own.into_iter().chain([colour]).collect())?;
            tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
                use lamp_gemini::Event;
                let started=lamp_ipc::monotonic_us();
                let mut connection=lamp_gemini::connect(config,lamp_gemini::SessionId::new(1)?).await?;
                let setup_us=lamp_ipc::monotonic_us()-started;
                let input=connection.input();
                input.try_written(lamp_gemini::RequestId::new(1)?,note,None)?;
                let asked=lamp_ipc::monotonic_us();
                let (mut said,mut samples,mut first,mut ended)=(String::new(),0usize,None,"timeout");
                let mut calls=Vec::new();
                // The order of what came, with the time of each, audio once.
                let mut order=Vec::new();
                let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(14);
                loop {
                    let event=tokio::time::timeout_at(deadline,connection.next_event()).await;
                    let at_ms=(lamp_ipc::monotonic_us()-asked)/1000;
                    let kind=match &event {
                        Ok(Some(Event::ToolCall{..}))=>Some("call"),
                        Ok(Some(Event::ToolCallsCancelled{..}))=>Some("calls_withdrawn"),
                        Ok(Some(Event::Audio{..})) if first.is_none()=>Some("first_audio"),
                        Ok(Some(Event::GenerationComplete{..}))=>Some("generation_complete"),
                        Ok(Some(Event::TurnComplete{..}))=>Some("turn_complete"),
                        Ok(Some(Event::Unanswered{..}))=>Some("unanswered"),
                        Ok(Some(Event::Discarded{..}))=>Some("discarded"),
                        Ok(Some(Event::WaitingForInput{..}))=>Some("waiting_for_input"),
                        _=>None,
                    };
                    if let Some(kind)=kind { order.push(format!("{kind}@{at_ms}")); }
                    match event {
                        Ok(Some(Event::ToolCall{call,..})) => {
                            let answer=if call.name==lamp_voice::tools::NOT_FOR_ME { lamp_voice::tools::declined() } else { serde_json::json!({"done":true}) };
                            let sent=input.try_tool_response(&call.id,&call.name,&answer).is_ok();
                            calls.push(serde_json::json!({"name":call.name,"args":call.args,"after_us":lamp_ipc::monotonic_us()-asked,"audio_samples_before":samples,"answer_sent":sent}));
                        }
                        Ok(Some(Event::ToolCallsCancelled{ids,..})) => calls.push(serde_json::json!({"withdrawn":ids.len()})),
                        Ok(Some(Event::Audio{pcm,..})) => { first.get_or_insert(lamp_ipc::monotonic_us()-asked); samples+=pcm.len(); }
                        Ok(Some(Event::OutputTranscript{text,..})) => said.push_str(&text),
                        Ok(Some(Event::TurnComplete{..})) => { ended="turn_complete"; break; }
                        Ok(Some(Event::Unanswered{..})) => { ended="unanswered"; break; }
                        Ok(Some(_)) => {}
                        Ok(None) => { ended="connection_ended"; break; }
                        Err(_) => break,
                    }
                }
                println!("{}",serde_json::json!({"status":"tools_tried","setup_us":setup_us,"ended":ended,"ended_after_us":lamp_ipc::monotonic_us()-asked,"order":order,"calls":calls,"first_audio_us":first,"audio_samples":samples,"said":said,"state":format!("{:?}",connection.state()),"microphone_opened":false,"audio_sent":false}));
                connection.shutdown();
                Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
            })?;
        },
        [command,path,flag,note] if command=="provider-check" && (flag=="--note" || flag=="--written") => {
            let config=ProviderConfig::load(Path::new(path))?.into_session()?;
            tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
                use lamp_gemini::Event;
                let started=lamp_ipc::monotonic_us();
                let mut connection=lamp_gemini::connect(config,lamp_gemini::SessionId::new(1)?).await?;
                let setup_us=lamp_ipc::monotonic_us()-started;
                let input=connection.input();
                let request=lamp_gemini::RequestId::new(1)?;
                if flag=="--written" {
                    input.try_written(request,note,None)?;
                } else {
                    input.try_start(request)?;
                    input.try_note(request,note)?;
                    input.try_end(request)?;
                }
                let asked=lamp_ipc::monotonic_us();
                let (mut said,mut samples,mut first,mut ended)=(String::new(),0usize,None,"timeout");
                let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(12);
                loop {
                    match tokio::time::timeout_at(deadline,connection.next_event()).await {
                        Ok(Some(Event::Audio{pcm,..})) => { first.get_or_insert(lamp_ipc::monotonic_us()-asked); samples+=pcm.len(); }
                        Ok(Some(Event::OutputTranscript{text,..})) => said.push_str(&text),
                        Ok(Some(Event::TurnComplete{..})) => { ended="turn_complete"; break; }
                        Ok(Some(Event::Unanswered{..})) => { ended="unanswered"; break; }
                        Ok(Some(_)) => {}
                        Ok(None) => { ended="connection_ended"; break; }
                        Err(_) => break,
                    }
                }
                println!("{}",serde_json::json!({"status":"note_tried","form":flag,"setup_us":setup_us,"ended":ended,"first_audio_us":first,"audio_samples":samples,"said":said,"state":format!("{:?}",connection.state()),"microphone_opened":false,"audio_sent":false}));
                connection.shutdown();
                Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
            })?;
        },
        // A note for the model beside a request's sound, as the worker gives
        // one when the lamp's name is heard: a recorded question is sent as if
        // heard (16 kHz, one channel, 16-bit little-endian, no header), the
        // note is offered half a second in, and the request ends. Whether the
        // link takes a note in that place, and what is answered. Nothing is
        // captured or played.
        [command,path,flag,note,sound] if command=="provider-check" && flag=="--note-in-speech" => {
            let config=ProviderConfig::load(Path::new(path))?.into_session()?;
            let samples:Vec<i16>=std::fs::read(sound)?.chunks_exact(2).map(|pair| i16::from_le_bytes([pair[0],pair[1]])).collect();
            tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
                use lamp_gemini::Event;
                let mut connection=lamp_gemini::connect(config,lamp_gemini::SessionId::new(1)?).await?;
                let input=connection.input();
                let request=lamp_gemini::RequestId::new(1)?;
                input.try_start(request)?;
                let mut noted=false;
                for (sequence,block) in samples.chunks(lamp_gemini::MAX_INPUT_SAMPLES).enumerate() {
                    input.try_audio(request,sequence as u64,std::time::Instant::now(),block)?;
                    if !noted && sequence>=25 {
                        input.try_note(request,note)?;
                        noted=true;
                    }
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                if !noted {
                    input.try_note(request,note)?;
                }
                input.try_end(request)?;
                let asked=lamp_ipc::monotonic_us();
                let (mut heard,mut said,mut audio,mut first,mut ended)=(String::new(),String::new(),0usize,None,"timeout");
                let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(12);
                loop {
                    match tokio::time::timeout_at(deadline,connection.next_event()).await {
                        Ok(Some(Event::Audio{pcm,..})) => { first.get_or_insert(lamp_ipc::monotonic_us()-asked); audio+=pcm.len(); }
                        Ok(Some(Event::OutputTranscript{text,..})) => said.push_str(&text),
                        Ok(Some(Event::UncorrelatedInputTranscript{text,..})) => heard.push_str(&text),
                        Ok(Some(Event::TurnComplete{..})) => { ended="turn_complete"; break; }
                        Ok(Some(Event::Unanswered{..})) => { ended="unanswered"; break; }
                        Ok(Some(_)) => {}
                        Ok(None) => { ended="connection_ended"; break; }
                        Err(_) => break,
                    }
                }
                println!("{}",serde_json::json!({"status":"note_in_speech_tried","sound_ms":samples.len()/16,"ended":ended,"heard":heard,"first_audio_us":first,"audio_samples":audio,"said":said,"state":format!("{:?}",connection.state()),"microphone_opened":false,"audio_played":false}));
                connection.shutdown();
                Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
            })?;
        },
        // Something told to the service that it should not answer, then a
        // second of near-silence sent as if heard, then a written request: does
        // a session take sound after such a fact, and still answer afterwards?
        // Nothing is captured or played.
        [command,path,flag,context,then,note] if command=="provider-check" && flag=="--context" && then=="--then-sound-and-written" => {
            let config=ProviderConfig::load(Path::new(path))?.into_session()?;
            tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
                use lamp_gemini::Event;
                let mut connection=lamp_gemini::connect(config,lamp_gemini::SessionId::new(1)?).await?;
                let input=connection.input();
                input.try_context(context)?;
                tokio::time::sleep(std::time::Duration::from_millis(500)).await;
                let heard=lamp_gemini::RequestId::new(1)?;
                input.try_start(heard)?;
                let mut noise=0x2545_f491u32;
                for sequence in 0..50u64 {
                    let block:Vec<i16>=(0..lamp_gemini::MAX_INPUT_SAMPLES).map(|_| { noise=noise.wrapping_mul(1_664_525).wrapping_add(1_013_904_223); ((noise>>24) as i16)-128 }).collect();
                    input.try_audio(heard,sequence,std::time::Instant::now(),&block)?;
                    tokio::time::sleep(std::time::Duration::from_millis(20)).await;
                }
                input.try_end(heard)?;
                let (mut sound_ended,mut sound_said,mut sound_samples)=("timeout",String::new(),0usize);
                let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(8);
                loop {
                    match tokio::time::timeout_at(deadline,connection.next_event()).await {
                        Ok(Some(Event::Audio{pcm,..})) => sound_samples+=pcm.len(),
                        Ok(Some(Event::OutputTranscript{text,..})) => sound_said.push_str(&text),
                        Ok(Some(Event::TurnComplete{..})) => { sound_ended="turn_complete"; break; }
                        Ok(Some(Event::Unanswered{..})) => { sound_ended="unanswered"; break; }
                        Ok(Some(_)) => {}
                        Ok(None) => { sound_ended="connection_ended"; break; }
                        Err(_) => break,
                    }
                }
                let state_after_sound=format!("{:?}",connection.state());
                let (mut said,mut samples,mut ended)=(String::new(),0usize,"not_tried");
                if sound_ended!="connection_ended" {
                    ended="timeout";
                    input.try_written(lamp_gemini::RequestId::new(2)?,note,None)?;
                    let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(12);
                    loop {
                        match tokio::time::timeout_at(deadline,connection.next_event()).await {
                            Ok(Some(Event::Audio{pcm,..})) => samples+=pcm.len(),
                            Ok(Some(Event::OutputTranscript{text,..})) => said.push_str(&text),
                            Ok(Some(Event::TurnComplete{..})) => { ended="turn_complete"; break; }
                            Ok(Some(Event::Unanswered{..})) => { ended="unanswered"; break; }
                            Ok(Some(_)) => {}
                            Ok(None) => { ended="connection_ended"; break; }
                            Err(_) => break,
                        }
                    }
                }
                println!("{}",serde_json::json!({"status":"context_then_sound_tried","sound_ended":sound_ended,"sound_said":sound_said,"sound_audio_samples":sound_samples,"state_after_sound":state_after_sound,"written_ended":ended,"written_said":said,"written_audio_samples":samples,"state":format!("{:?}",connection.state()),"microphone_opened":false,"audio_played":false}));
                connection.shutdown();
                Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
            })?;
        },
        // Something told to the service that it should not answer, and then a
        // written request that it should: whether the first is taken in, and
        // whether anything comes back for it. Nothing is captured or played.
        [command,path,flag,context,then,note] if command=="provider-check" && flag=="--context" && then=="--written" => {
            let config=ProviderConfig::load(Path::new(path))?.into_session()?;
            tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
                use lamp_gemini::Event;
                let mut connection=lamp_gemini::connect(config,lamp_gemini::SessionId::new(1)?).await?;
                let input=connection.input();
                input.try_context(context)?;
                // Three seconds for anything to come back for the context alone.
                let (mut back_samples,mut back_said)=(0usize,String::new());
                let quiet_until=tokio::time::Instant::now()+std::time::Duration::from_secs(3);
                while let Ok(Some(event))=tokio::time::timeout_at(quiet_until,connection.next_event()).await {
                    match event {
                        Event::Audio{pcm,..} => back_samples+=pcm.len(),
                        Event::OutputTranscript{text,..} => back_said.push_str(&text),
                        _ => {}
                    }
                }
                input.try_written(lamp_gemini::RequestId::new(1)?,note,None)?;
                let asked=lamp_ipc::monotonic_us();
                let (mut said,mut samples,mut first,mut ended)=(String::new(),0usize,None,"timeout");
                let deadline=tokio::time::Instant::now()+std::time::Duration::from_secs(12);
                loop {
                    match tokio::time::timeout_at(deadline,connection.next_event()).await {
                        Ok(Some(Event::Audio{pcm,..})) => { first.get_or_insert(lamp_ipc::monotonic_us()-asked); samples+=pcm.len(); }
                        Ok(Some(Event::OutputTranscript{text,..})) => said.push_str(&text),
                        Ok(Some(Event::TurnComplete{..})) => { ended="turn_complete"; break; }
                        Ok(Some(Event::Unanswered{..})) => { ended="unanswered"; break; }
                        Ok(Some(_)) => {}
                        Ok(None) => { ended="connection_ended"; break; }
                        Err(_) => break,
                    }
                }
                println!("{}",serde_json::json!({"status":"context_tried","after_context_audio_samples":back_samples,"after_context_said":back_said,"ended":ended,"first_audio_us":first,"audio_samples":samples,"said":said,"state":format!("{:?}",connection.state()),"microphone_opened":false,"audio_played":false}));
                connection.shutdown();
                Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
            })?;
        },
        [command,path] if command=="provider-check" => {
            let config=ProviderConfig::load(Path::new(path))?.into_session()?;
            tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(async {
                let started=lamp_ipc::monotonic_us();
                let connection=lamp_gemini::connect(config,lamp_gemini::SessionId::new(1)?).await?;
                println!("{}",serde_json::json!({"status":"setup_complete","setup_us":lamp_ipc::monotonic_us()-started,"microphone_opened":false,"audio_sent":false}));
                connection.shutdown();
                Ok::<(),Box<dyn std::error::Error+Send+Sync>>(())
            })?;
        },
        #[cfg(target_os = "linux")]
        [command, config, seconds, output] if command == "camera-inspect" => {
            lamp_voice::camera_inspect::run(Path::new(config), seconds.parse()?, Path::new(output))?;
        }
        #[cfg(target_os = "linux")]
        [command, role, dir, parent, worker, config, digest, flags @ ..] if command == "worker" && role == "camera" => {
            let parent = parse_boot(parent)?;
            let worker = parse_boot(worker)?;
            let mut channels = WorkerChannels::bind(Path::new(dir), role, parent, worker, true)?;
            channels.connect(Path::new(dir), role, true)?;
            let consumers = lamp_voice::camera_worker::parse_frame_consumers(flags)?;
            lamp_voice::camera_worker::run(channels, parent, worker, Path::new(config), digest, Path::new(dir), &consumers)?;
        }
        #[cfg(target_os = "linux")]
        [command, wav, sha256, seconds, output, flags @ ..] if command == "directed-fixture" => {
            let options = lamp_voice::fixture_provider::FixtureOptions::parse(flags)?;
            lamp_voice::coordinator::run_fixture(Path::new(wav), sha256, seconds.parse()?, Path::new(output), options)?;
        }
        [command, role, dir, parent, worker, flag, wav, sha256]
            if command == "worker" && role == "provider" && flag == "--fixture" => {
            let parent = parse_boot(parent)?;
            let worker = parse_boot(worker)?;
            let mut channels = WorkerChannels::bind(Path::new(dir), role, parent, worker, true)?;
            channels.connect(Path::new(dir), role, true)?;
            lamp_voice::fixture_provider::run(channels, parent, Path::new(wav), sha256)?;
        }
        #[cfg(target_os = "linux")]
        [command, config, output, flags @ ..] if command == "serve" => {
            let options = lamp_voice::options::SessionOptions::parse(flags)?;
            options.validate_serve()?;
            lamp_voice::coordinator::run_serve(Path::new(config), Path::new(output), options)?;
        }
        #[cfg(target_os = "linux")]
        [command, config, seconds, output, flags @ ..] if command == "directed" => {
            let options = lamp_voice::options::SessionOptions::parse(flags)?;
            lamp_voice::coordinator::run_directed_session(Path::new(config), seconds.parse()?, Path::new(output), options)?;
        }
        #[cfg(target_os = "linux")]
        [command, role, dir, parent, worker, device, flags @ ..]
            if command == "worker" && matches!(role.as_str(), "capture" | "speaker") => {
            let options = lamp_voice::options::AudioWorkerOptions::parse(flags, role == "capture")?;
            let parent = parse_boot(parent)?;
            let worker = parse_boot(worker)?;
            let mut channels = WorkerChannels::bind(Path::new(dir), role, parent, worker, true)?;
            channels.connect(Path::new(dir), role, true)?;
            if role == "capture" {
                lamp_voice::audio_workers::capture_with_options(channels,parent,worker,Path::new(dir),device,&options)?;
            } else {
                lamp_voice::audio_workers::speaker(channels,parent,worker,Path::new(dir),device,options.diagnostic_directory.as_deref())?;
            }
        }
        [command, role, dir, parent, worker, extra, flags @ ..] if command == "worker" && role == "provider" => {
            let parent = parse_boot(parent)?;
            let worker = parse_boot(worker)?;
            let mut channels = WorkerChannels::bind(Path::new(dir), role, parent, worker, true)?;
            channels.connect(Path::new(dir), role, true)?;
            match role.as_str() {
                "provider" => {
                    let (continuation, flags) = lamp_voice::provider_worker::Continuation::take(flags)?;
                    let (sight, flags) = lamp_voice::sight::Sight::take(flags)?;
                    let (look_file, flags) = lamp_voice::sight::take_look(flags)?;
                    let (thread_file, flags) = lamp_voice::thread::take_file(flags)?;
                    let (session_fact, flags) = lamp_voice::session_fact::take(flags);
                    let (room_facts, flags) = lamp_voice::room_told::take(flags);
                    let (air_facts, flags) = lamp_voice::air_told::take(flags);
                    let (keep_link, flags) = lamp_voice::provider_worker::take_keep_link(flags);
                    let (tools, flags) = lamp_voice::tools::take(flags)?;
                    let (attention, flags) = lamp_voice::attention::take(flags);
                    let (attention_busy, flags) = lamp_voice::attention::take_busy(flags);
                    let (attention_near, flags) = lamp_voice::attention::take_near(flags);
                    let (addressed, flags) = lamp_voice::addressed::take(flags);
                    let (name_signal, flags) = lamp_voice::attention::take_name(flags);
                    let (name_note, name_note_busy_face, flags) = lamp_voice::called::take(flags);
                    let (bus_root, flags) = lamp_voice::bus_out::take_root(flags)?;
                    let (first_output, flags) = lamp_voice::first_output::take(flags)?;
                    let visual = lamp_voice::visual_context::Source::parse(Path::new(dir), flags)?.map(|source| (source, worker));
                    tokio::runtime::Builder::new_current_thread().enable_all().build()?.block_on(lamp_voice::provider_worker::run_with_continuation(channels,parent,Path::new(extra),lamp_voice::provider_worker::Reporting::Coordinated,visual,continuation,sight,lamp_voice::provider_worker::Files{look:look_file,thread:thread_file,session_fact,room_facts,air_facts,keep_link,tools,attention,attention_busy,attention_near,addressed,name_signal,name_note,name_note_busy_face,bus_root,first_output}))?;
                },
                _ => return Err(io::Error::other("unknown worker role").into()),
            }
        }
        // worker ring DIR PARENT WORKER --continuous [--resting CEILING] [--bus-root DIR] [--eye]
        #[cfg(target_os = "linux")]
        [command, role, dir, parent, worker, mode, flags @ ..] if command == "worker" && role == "ring" && mode == "--continuous" => {
            let parent = parse_boot(parent)?;
            let worker = parse_boot(worker)?;
            let (ceiling, flags) = match flags {
                [flag, ceiling, rest @ ..] if flag == "--resting" => {
                    let ceiling: u16 = ceiling.parse().map_err(|_| io::Error::new(io::ErrorKind::InvalidInput, "resting ceiling must be 0 to 120"))?;
                    (Some(ceiling), rest)
                }
                rest => (None, rest),
            };
            let (bus_root, flags) = lamp_voice::bus_out::take_root(flags)?;
            let (eye, flags) = lamp_voice::ring_sight::take(flags);
            if !flags.is_empty() {
                return Err(io::Error::new(io::ErrorKind::InvalidInput, "unknown ring worker option").into());
            }
            let mut channels = WorkerChannels::bind(Path::new(dir), role, parent, worker, true)?;
            channels.connect(Path::new(dir), role, true)?;
            lamp_voice::ring_worker::run_continuous(channels, parent, ceiling, bus_root.as_deref(), eye)?;
        }
        [command,role,dir,parent,worker] if command=="worker" => {
            let parent=parse_boot(parent)?;
            let worker=parse_boot(worker)?;
            let mut channels=WorkerChannels::bind(Path::new(dir),role,parent,worker,true)?;
            channels.connect(Path::new(dir),role,true)?;
            match role.as_str() {
                #[cfg(target_os="linux")]
                "privacy"=>lamp_voice::physical_privacy::run(channels)?,
                #[cfg(target_os="linux")]
                "ring"=>lamp_voice::ring_worker::run(channels,parent)?,
                "probe"=>probe(channels)?,
                _=>return Err(io::Error::other("worker is not integrated yet").into()),
            }
        },
        _=>return Err(io::Error::new(io::ErrorKind::InvalidInput,"usage: lamp-voice serve PRIVATE_CONFIG.json NEW_OUTPUT [directed options except --diagnostics] | camera-inspect PRIVATE_CAMERA_CONFIG.json SECONDS NEW_REPORT.json | provider-check PRIVATE_CONFIG.json | directed PRIVATE_CONFIG.json SECONDS NEW_OUTPUT [--controls-stop] [--controls-mute ABS_PRIVATE_RECORD] [--controls-bus-root ABS_PATH] [--camera-config ABS_PATH] [--motor-worker ABS_PATH --motor-config ABS_PATH] [--head-qualification-plan ABS_PATH --head-qualification-channel ABS_PRIVATE_DIR] [--failure-notice-dir ABS_PATH --failure-notice-language TAG] [--diagnostics] [--noise-suppression on|off] [--microphone mic1|mic2] [--turn-taking v1-baseline|vad-only|duplex] [--reference-clock fixed|capture] [--reference-recovery off|bounded] [--echo-reference as-given|steady] [--provider-continuation off|kept-start] [--playout-start-ms 0..500 | --playout-continuity] [--cue-socket ABS_PATH] [--ring-channel-ceiling 0..120] [--end-silence-ms 300..2000] [--early-end-ms 100..1000, with --provider-continuation kept-start] | directed-fixture REPLY.wav SHA256 SECONDS NEW_OUTPUT --diagnostics [--noise-suppression on|off] [--microphone mic1|mic2] [--turn-taking v1-baseline|vad-only|duplex] [--reference-clock fixed|capture] [--reference-recovery off|bounded] [--playout-start-ms 0..500 | --playout-continuity] [--cue-socket ABS_PATH] [--ring-channel-ceiling 0..120] | worker ROLE DIR CONTROLLER_BOOT WORKER_BOOT").into()),
    }
    Ok(())
}

fn probe(mut channels: WorkerChannels) -> Result<()> {
    use lamp_voice::wire::{Control, WorkerEvent};
    channels.control.send(WorkerEvent::Ready)?;
    let mut last = lamp_ipc::monotonic_us();
    loop {
        match channels.control.receive()? {
            Some(Control::Stop) => return Ok(()),
            Some(Control::Authority { .. }) => {
                last = lamp_ipc::monotonic_us();
                channels.control.send(WorkerEvent::Ready)?;
            }
            Some(
                Control::StartCapture
                | Control::ConnectReference { .. }
                | Control::ProviderOutputCapacity { .. },
            ) => {
                return Err(io::Error::other("invalid probe command").into());
            }
            None => {}
        }
        if lamp_ipc::monotonic_us().saturating_sub(last) > 250_000 {
            return Err(io::Error::other("controller heartbeat expired").into());
        }
        std::thread::sleep(std::time::Duration::from_millis(2));
    }
}
