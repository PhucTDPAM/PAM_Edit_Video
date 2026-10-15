// SPDX-License-Identifier: AGPL-3.0-or-later
// SPDX-FileCopyrightText: 2026 Jareer and Concat contributors

//! The export sheet: the form, the running render, and the result.
//!
//! The first pane cut out of the window's controller, and the shape every
//! later one follows. The pane owns its state; every way it can change -
//! a field edited, a button pressed, a worker reporting - is one
//! [`ExportMsg`]; [`ExportPane::update`] is the only code that changes the
//! state; and [`ExportPane::data`] is the one place its Slint rows are
//! built. The controller it needs for context - the session, the output
//! size, the job slots - is handed to `update` as the studio, which the
//! pane reads and asks things of but never reaches into for its own
//! fields.

use pamedit_host::export::{self, ExportSpec};
use pamedit_media::ColorRange;

use crate::format::{bytes, eta};
use crate::host::{on_ui, spawn};
use crate::i18n::{self, t, tf};
use crate::panes::Msg;
use crate::platform;
use crate::studio::{
    AUDIO_BPS, EXPORT_CRF, EXPORT_SHORT_SIDES, EXPORT_TIERS, RATES, Studio, home_folder,
};
use crate::ui::{ExportData, ExportPhase, QueueRow};
use slint::{ModelRc, SharedString, VecModel};

/// Everything that can happen to the export sheet.
#[derive(Clone, Debug)]
pub enum ExportMsg {
    /// The sheet is asked for: the menu, the tray button or ⌘E.
    Open,
    /// The sheet is dismissed.
    Close,
    NameEdited(String),
    ResolutionChanged(i32),
    RateChanged(i32),
    QualityChanged(i32),
    CodecChanged(i32),
    TenBitChanged(bool),
    /// The GPU's encoder for every codec, or x264 for H.264 as before.
    GpuEncodeChanged(bool),
    /// For an HDR timeline: written HDR (true), or tone-mapped to SDR.
    HdrChanged(bool),
    /// Limited or full range, as a row of the Advanced section's list.
    ColorRangeChanged(i32),
    /// The Advanced section is opened or closed.
    AdvancedToggled(bool),
    /// VBR or CBR.
    RateModeChanged(i32),
    /// The target bitrate field: digits only, in kbps.
    BitrateChanged(String),
    /// Back to the form after a finished or failed render.
    Again,
    /// Pick the destination folder.
    Browse,
    /// The folder the picker answered with.
    FolderChosen(String),
    /// Show the finished file in the file manager.
    Reveal,
    Start,
    /// Start over a file already there: the confirmation sheet's answer.
    Overwrite,
    Cancel,
    /// The render's worker reporting where it is.
    Progress {
        /// Of the whole, `0..=1`.
        fraction: f32,
        /// What it is doing, in the person's language.
        stage: String,
    },
    /// The render's worker is done: the file written, or why not.
    Finished(Result<String, String>),
    /// The phone has moved the file to where it shows it - the path under
    /// the phone's storage - or could not, and the file is where it was.
    Published(Result<String, String>),
    /// PAM_Edit Video: the sheet's settings, for this project, put at the
    /// end of the render queue.
    Enqueue,
    /// Ctrl+M with the sheet up: queued, and the sheet out of the way, the
    /// way Premiere's Queue button hands off to Media Encoder.
    QuickEnqueue,
    /// A queued render's worker reporting where it is.
    QueueProgress { id: u64, fraction: f32 },
    /// A queued render's worker is done.
    QueueFinished {
        id: u64,
        result: Result<String, String>,
    },
    /// A queue row's close: a waiting render leaves the queue, the running
    /// one is stopped, a finished one is cleared.
    QueueRemove(i32),
    /// Every finished, failed or stopped row cleared.
    QueueClearFinished,
    /// PAM_Edit Video: one of the sheet's file and render options, by key,
    /// as the sheet's generic `option` callback reports it.
    OptionChanged { key: String, value: String },
}

/// Where a queued render is.
#[derive(Clone, Debug, PartialEq)]
pub enum QueueState {
    Waiting,
    Running,
    Done,
    Failed(String),
    Cancelled,
}

/// One render in the queue. The request is the project as it was when it
/// was queued - flattened, titles painted - so the project can be edited,
/// closed or swapped for another while it waits and while it renders.
pub struct QueueItem {
    id: u64,
    /// The project it was queued from.
    project: String,
    /// The file it writes.
    output: String,
    /// Taken by the worker when the render starts.
    request: Option<pamedit_export::ExportRequest>,
    state: QueueState,
    progress: f32,
}

/// The export sheet's state.
pub struct ExportPane {
    pub open: bool,
    pub name: String,
    pub folder: String,
    pub resolution: usize,
    pub rate: usize,
    pub quality: usize,
    /// Index into `VideoCodec::ALL`.
    pub codec: usize,
    pub ten_bit: bool,
    /// Encode on the GPU (NVENC and its kin) for every codec. On unless
    /// turned off: a desk with an NVIDIA, Intel or AMD chip exports
    /// several times faster, and one without falls back to the CPU.
    pub gpu_encode: bool,
    /// An HDR timeline written tone-mapped to SDR rather than as it is.
    pub sdr: bool,
    /// Index into `ColorRange::ALL`: 0 limited, 1 full. Read only while
    /// the Advanced section is open, like the bitrate.
    /// upstream #103
    pub color_range: usize,
    /// The Advanced section is open: bitrate controls show, and the size
    /// estimate reads the chosen bitrate.
    pub advanced: bool,
    /// Index into `RateMode::ALL`.
    pub rate_mode: usize,
    /// Target bitrate in kbps, used when `rate_mode` is CBR.
    pub bitrate: u32,
    pub phase: ExportPhase,
    pub progress: f32,
    pub stage: String,
    pub message: String,
    /// Where the finished file is, for Reveal.
    pub written: String,
    /// When the render started, for a real ETA.
    started_at: Option<std::time::Instant>,
    /// The render queue: rendered top to bottom, one at a time, while the
    /// window goes on with anything else. Not the project's: it outlives
    /// the project each row was queued from.
    queue: Vec<QueueItem>,
    next_queue_id: u64,
    /// PAM_Edit Video: a frame size of the person's own, for the rung past
    /// the ladder's end.
    custom_width: u32,
    custom_height: u32,
    /// What the target-size field holds, in MB, as typed.
    target_mb: String,
    /// The replace-the-file question was asked for a queueing, not for an
    /// export now.
    confirming_queue: bool,
}

impl Default for ExportPane {
    fn default() -> Self {
        Self {
            open: false,
            name: "Untitled".into(),
            folder: home_folder("Movies"),
            resolution: 2,
            rate: 0,
            quality: 1,
            codec: 0,
            ten_bit: false,
            gpu_encode: true,
            sdr: false,
            color_range: 0,
            advanced: false,
            rate_mode: 0,
            bitrate: 8000,
            phase: ExportPhase::Idle,
            progress: 0.0,
            stage: String::new(),
            message: String::new(),
            written: String::new(),
            started_at: None,
            queue: Vec::new(),
            next_queue_id: 1,
            custom_width: 1920,
            custom_height: 1080,
            target_mb: String::new(),
            confirming_queue: false,
        }
    }
}

impl ExportPane {
    /// Applies one message. The studio is the rest of the window, for
    /// what the sheet needs to know and to start; the pane's own state is
    /// `self`, and while this runs the studio's copy of it is a blank the
    /// pane must not read.
    pub fn update(&mut self, msg: ExportMsg, studio: &mut Studio) {
        self.apply(msg, studio);
        // Whatever happened, the next queued render starts if the exporter
        // is free: after a queueing, a queued render's end, and an export
        // now that finished or was stopped.
        self.pump(studio);
    }

    fn apply(&mut self, msg: ExportMsg, studio: &mut Studio) {
        match msg {
            ExportMsg::Open => {
                self.open = true;
                self.phase = ExportPhase::Idle;
                self.message.clear();
                // The timeline's own size: the one rung that can be picked;
                // and its own rate, first on the rate list.
                self.resolution = Self::own_rung(studio);
                self.rate = 0;
            }
            ExportMsg::Close => self.open = false,
            ExportMsg::NameEdited(name) => self.name = name,
            ExportMsg::ResolutionChanged(index) => {
                // PAM_Edit Video: any rung, or the custom size one past them.
                let index = index.max(0) as usize;
                if index <= Self::ladder(studio).len() {
                    self.resolution = index;
                    if self.is_custom(studio) {
                        let (width, height) = studio.output_size();
                        if self.custom_width == 0 || self.custom_height == 0 {
                            self.custom_width = width;
                            self.custom_height = height;
                        }
                    }
                }
            }
            ExportMsg::RateChanged(index) => {
                self.rate = (index.max(0) as usize).min(Self::rates(studio).len() - 1);
            }
            ExportMsg::QualityChanged(index) => self.quality = (index.max(0) as usize).min(2),
            // A codec this device cannot encode is not taken: the picker
            // says so under it, and the choice before stands.
            ExportMsg::CodecChanged(index) => {
                let index = (index.max(0) as usize).min(pamedit_media::VideoCodec::ALL.len() - 1);
                if pamedit_media::VideoCodec::ALL[index].encodable() {
                    self.codec = index;
                }
            }
            ExportMsg::TenBitChanged(on) => self.ten_bit = on,
            ExportMsg::GpuEncodeChanged(on) => self.gpu_encode = on,
            ExportMsg::HdrChanged(on) => self.sdr = !on,
            ExportMsg::ColorRangeChanged(index) => {
                self.color_range = (index.max(0) as usize).min(ColorRange::ALL.len() - 1);
            }
            ExportMsg::AdvancedToggled(on) => self.advanced = on,
            ExportMsg::RateModeChanged(index) => self.rate_mode = index.max(0) as usize,
            ExportMsg::BitrateChanged(text) => {
                let digits: String = text.chars().filter(|c| c.is_ascii_digit()).collect();
                if let Ok(value) = digits.parse::<u32>() {
                    self.bitrate = value.clamp(100, 200_000);
                }
            }
            ExportMsg::Again => {
                self.phase = ExportPhase::Idle;
                self.progress = 0.0;
            }
            // The picker runs with the studio unborrowed and answers as a
            // message of its own; see `host::after_dialog`.
            ExportMsg::Browse => {
                let title = i18n::t("export.exportTo");
                let current = self.folder.clone();
                crate::host::after_dialog(
                    move || platform::pick_folder(&title, &current),
                    |studio, _, _, folder| {
                        studio.handle(Msg::Export(ExportMsg::FolderChosen(
                            folder.to_string_lossy().into_owned(),
                        )));
                    },
                );
            }
            ExportMsg::FolderChosen(folder) => self.folder = folder,
            ExportMsg::Reveal => {
                if !self.written.is_empty()
                    && let Err(error) = platform::reveal(&self.written)
                {
                    studio.notify(&i18n::tf("export.couldNotShowFile", &[&error]), true);
                }
            }
            // While the queue renders, Export joins it rather than
            // being refused.
            ExportMsg::Start if self.queue_busy() => self.enqueue(studio, false),
            ExportMsg::Start => self.start(studio),
            ExportMsg::Overwrite => {
                if std::mem::take(&mut self.confirming_queue) {
                    self.enqueue(studio, true);
                } else {
                    self.begin(studio, true);
                }
            }
            ExportMsg::Enqueue => self.enqueue(studio, false),
            ExportMsg::QuickEnqueue => {
                let before = self.queue.len();
                self.enqueue(studio, false);
                // Closed only when it went in: a refusal stays on the sheet
                // to be read, and a replace-the-file question to be answered.
                if self.queue.len() > before {
                    self.open = false;
                }
            }
            ExportMsg::QueueProgress { id, fraction } => {
                if let Some(item) = self.queue.iter_mut().find(|item| item.id == id)
                    && item.state == QueueState::Running
                {
                    item.progress = fraction.clamp(0.0, 1.0);
                }
            }
            ExportMsg::QueueFinished { id, result } => {
                let Some(item) = self.queue.iter_mut().find(|item| item.id == id) else {
                    return;
                };
                let name = file_stem(&item.output);
                match result {
                    Ok(written) => {
                        item.state = QueueState::Done;
                        item.progress = 1.0;
                        when_rendered(studio, &written);
                        studio.notify(&tf("export.queueItemFinished", &[&name]), false);
                    }
                    // Stopped from its row: it says so already.
                    Err(_) if item.state == QueueState::Cancelled => {}
                    Err(error) => {
                        item.state = QueueState::Failed(error.clone());
                        studio.notify(&tf("export.queueItemFailed", &[&name, &error]), true);
                    }
                }
            }
            ExportMsg::QueueRemove(index) => {
                let Some(item) = usize::try_from(index)
                    .ok()
                    .and_then(|index| self.queue.get_mut(index))
                else {
                    return;
                };
                if item.state == QueueState::Running {
                    // Stops at the next frame; its row stays, saying so.
                    studio.host.exporter.cancel();
                    item.state = QueueState::Cancelled;
                } else {
                    self.queue.remove(index as usize);
                }
            }
            ExportMsg::OptionChanged { key, value } => self.set_option(studio, &key, &value),
            ExportMsg::QueueClearFinished => {
                self.queue
                    .retain(|item| matches!(item.state, QueueState::Waiting | QueueState::Running));
            }
            ExportMsg::Cancel => {
                studio.host.exporter.cancel();
                self.phase = ExportPhase::Idle;
                self.progress = 0.0;
            }
            ExportMsg::Progress { fraction, stage } => {
                if self.phase == ExportPhase::Running {
                    self.progress = fraction.clamp(0.0, 1.0);
                    self.stage = stage;
                }
            }
            ExportMsg::Finished(Ok(written)) => {
                self.phase = ExportPhase::Done;
                self.progress = 1.0;
                self.written = written.clone();
                when_rendered(studio, &written);
                // A phone moves the file to where its gallery shows it, and
                // the sheet says "finished" once it is there.
                let handed = platform::publish_export(written.into(), |result| {
                    on_ui(move |studio, _, _| {
                        studio.handle(Msg::Export(ExportMsg::Published(result)));
                    });
                });
                if !handed {
                    studio.notify(&t("export.exportFinished"), false);
                }
            }
            ExportMsg::Published(Ok(path)) => {
                self.written = path;
                studio.notify(&t("export.exportFinished"), false);
            }
            ExportMsg::Published(Err(error)) => {
                let folder = platform::published_folder().unwrap_or_default();
                studio.notify(&tf("export.couldNotPublish", &[&folder, &error]), true);
            }
            ExportMsg::Finished(Err(error)) => {
                if self.phase == ExportPhase::Idle {
                    // Cancelled: the sheet already went back to idle.
                    return;
                }
                self.phase = ExportPhase::Failed;
                self.message = error.clone();
                studio.notify(&tf("export.exportFailed", &[&error]), true);
            }
        }
    }

    /// The resolution ladder for this project: the standard short sides,
    /// with the project's own among them where it is not already - a
    /// custom frame has its own rung - from largest to smallest.
    fn ladder(studio: &Studio) -> Vec<u32> {
        let mut rungs: Vec<u32> = EXPORT_SHORT_SIDES.to_vec();
        let own = Self::own_short(studio);
        if !rungs.contains(&own) {
            rungs.push(own);
        }
        rungs.sort_unstable_by(|a, b| b.cmp(a));
        rungs
    }

    /// The rates the sheet offers: the timeline's own first - what it is
    /// exported at unless the person picks another - then every other
    /// rate a project can be made at. A 25 fps cut used to export at 30
    /// unless someone noticed (audit 2026-10-04, #8).
    fn rates(studio: &Studio) -> Vec<(i64, i64)> {
        let video = studio.project().active().video;
        Self::rates_around((video.rate_num, video.rate_den.max(1)))
    }

    /// [`ExportPane::rates`] for a timeline at `own`.
    fn rates_around(own: (i64, i64)) -> Vec<(i64, i64)> {
        let same = |(num, den): (i64, i64)| num * own.1 == own.0 * den;
        std::iter::once(own)
            .chain(
                RATES
                    .iter()
                    .map(|&(_, num, den)| (num, den))
                    .filter(|&rate| !same(rate)),
            )
            .collect()
    }

    /// The rate the export renders at.
    fn rate(&self, studio: &Studio) -> (i64, i64) {
        let rates = Self::rates(studio);
        rates[self.rate.min(rates.len() - 1)]
    }

    /// A rate as the sheet names it: "23.976 fps", "25 fps".
    fn rate_name((num, den): (i64, i64)) -> String {
        let name = match RATES.iter().find(|&&(_, n, d)| n * den == num * d) {
            Some((name, _, _)) => (*name).to_owned(),
            None => {
                let fps = format!("{:.3}", crate::studio::fps_of(num, den));
                fps.trim_end_matches('0').trim_end_matches('.').to_owned()
            }
        };
        format!("{name} fps")
    }

    /// The project's short side, in pixels.
    fn own_short(studio: &Studio) -> u32 {
        let (width, height) = studio.output_size();
        width.min(height).max(2)
    }

    /// The rung of the ladder that is the project's own size: the one
    /// the sheet lets be picked, since an edit exports at the size it was
    /// cut at. upstream #109
    fn own_rung(studio: &Studio) -> usize {
        let own = Self::own_short(studio);
        Self::ladder(studio)
            .iter()
            .position(|rung| *rung == own)
            .unwrap_or(0)
    }

    /// `short` scaled along the project's aspect and rounded to even
    /// dimensions, which is what the encoder's chroma subsampling needs.
    fn frame_for(studio: &Studio, short: u32) -> (u32, u32) {
        let (project_w, project_h) = studio.output_size();
        let (project_w, project_h) = (project_w.max(1) as f64, project_h.max(1) as f64);
        let even = |side: f64| ((side / 2.0).round() as u32 * 2).max(2);
        if project_w >= project_h {
            (even(short as f64 * project_w / project_h), short)
        } else {
            (short, even(short as f64 * project_h / project_w))
        }
    }

    /// The frame the export renders at: the sheet's rung of the ladder,
    /// along the project's aspect.
    pub fn size(&self, studio: &Studio) -> (u32, u32) {
        if self.is_custom(studio) {
            let even = |side: u32| (side.clamp(16, 8192) / 2) * 2;
            return (even(self.custom_width), even(self.custom_height));
        }
        let ladder = Self::ladder(studio);
        let short = ladder[self.resolution.min(ladder.len() - 1)];
        Self::frame_for(studio, short)
    }

    /// A rough size of the file at one quality tier, in bytes.
    pub fn size_bytes(&self, studio: &Studio, tier: usize) -> f32 {
        let (width, height) = self.size(studio);
        let (num, den) = self.rate(studio);
        let rate = num as f32 / den as f32;
        let pixels = (width as f32 * height as f32) / (1920.0 * 1080.0);
        // In CBR the bitrate is the number, not the tier; the pixels, rate
        // and codec factors no longer apply because the encoder is pinned.
        let video = if self.advanced && self.rate_mode == 1 {
            self.bitrate as f32 * 1000.0
        } else {
            EXPORT_TIERS[tier.min(2)]
                * 1_000_000.0
                * pixels
                * (rate / 30.0)
                * self.codec(studio).size_factor()
                * if self.deep(studio) { 1.05 } else { 1.0 }
        };
        let _ = AUDIO_BPS;
        (video + studio.prefs.export_options.audio_kbps as f32 * 1000.0) * studio.duration().max(1.0)
            / 8.0
    }

    /// The codec the sheet has chosen - HEVC for an HDR file where H.264
    /// was, since H.264 does not carry HDR.
    pub fn codec(&self, studio: &Studio) -> pamedit_media::VideoCodec {
        let codec =
            pamedit_media::VideoCodec::ALL[self.codec.min(pamedit_media::VideoCodec::ALL.len() - 1)];
        if self.hdr(studio) && codec == pamedit_media::VideoCodec::H264 {
            pamedit_media::VideoCodec::Hevc
        } else {
            codec
        }
    }

    /// Whether the file is written HDR: the timeline is, the sheet was
    /// not told to tone-map it to SDR, and the codec an HDR file would be
    /// written in can be written at ten bits here - on a phone it cannot,
    /// and the file is tone-mapped rather than failing.
    pub fn hdr(&self, studio: &Studio) -> bool {
        let chosen =
            pamedit_media::VideoCodec::ALL[self.codec.min(pamedit_media::VideoCodec::ALL.len() - 1)];
        let deep = match chosen {
            pamedit_media::VideoCodec::H264 => pamedit_media::VideoCodec::Hevc,
            codec => codec,
        };
        studio.timeline().video.color_space.is_hdr()
            && !self.sdr
            && deep.ten_bit_available()
            && deep.encodable()
    }

    /// Whether the file is written at ten bits: HDR always, SDR when the
    /// switch asks and the codec can be written that deep here.
    pub fn deep(&self, studio: &Studio) -> bool {
        self.hdr(studio) || (self.ten_bit && self.codec(studio).ten_bit_available())
    }

    /// The range the file is written in: the Advanced section's choice
    /// while the section is open, video range otherwise - so a sheet with
    /// Advanced off exports what it always did.
    pub fn color_range(&self) -> ColorRange {
        if self.advanced {
            ColorRange::ALL[self.color_range.min(ColorRange::ALL.len() - 1)]
        } else {
            ColorRange::Limited
        }
    }

    /// Starts the render on a worker, once the name is a file's and no
    /// file is already there; its reports come back as messages.
    fn start(&mut self, studio: &mut Studio) {
        self.begin(studio, false);
    }

    /// [`ExportPane::start`], with `overwrite` saying the person has
    /// answered the sheet that asked about the file already there.
    fn begin(&mut self, studio: &mut Studio, overwrite: bool) {
        self.confirming_queue = false;
        let Some((_, request)) = self.prepare(studio, overwrite) else {
            return;
        };
        let job = match studio.host.exporter.begin() {
            Ok(job) => job,
            Err(error) => {
                self.phase = ExportPhase::Failed;
                self.message = if self.queue_busy() {
                    t("export.queueIsRendering")
                } else {
                    error
                };
                return;
            }
        };

        studio.pause();
        self.phase = ExportPhase::Running;
        self.progress = 0.0;
        self.stage = t("export.renderingVideo");
        self.message.clear();
        self.written.clear();
        self.started_at = Some(std::time::Instant::now());

        // The export draws on the window's device - a sibling of the
        // monitor's compositor - not on a second device opened through the
        // drivers for the export alone, which on Windows laptops with NVIDIA
        // chips took the whole app down at the first frame with nothing in
        // the log (issue #202). Not on Vulkan, whose queue the window
        // submits to from its own thread (#324); see `Monitor::off_thread`.
        // None there or without a GPU, and the render then opens what it
        // can.
        let compositor = studio.host.monitor.off_thread();
        let low = studio.prefs.export_options.low_priority;
        spawn(
            move || {
                let job = job;
                if low {
                    lower_thread_priority();
                }
                export::run_on(&request, compositor, job.cancel_flag(), |progress| {
                    let fraction = if progress.total > 0 {
                        progress.frame as f32 / progress.total as f32
                    } else {
                        0.0
                    };
                    let stage = match progress.stage {
                        "rendering" => t("export.renderingVideo"),
                        "mixing audio" => t("export.mixingAudio"),
                        "muxing" => t("export.finalisingFile"),
                        other => other.to_owned(),
                    };
                    on_ui(move |studio, _, _| {
                        studio.handle(Msg::Export(ExportMsg::Progress { fraction, stage }));
                    });
                })
            },
            |studio, _, _, result| studio.handle(Msg::Export(ExportMsg::Finished(result))),
        );
    }

    /// The sheet's settings put at the end of the queue, the project as it
    /// stands now. The sheet stays as it was, ready for the next project.
    fn enqueue(&mut self, studio: &mut Studio, overwrite: bool) {
        // Set while asking, so the replace-the-file answer comes back here.
        self.confirming_queue = true;
        let Some((output, request)) = self.prepare(studio, overwrite) else {
            if studio.confirm.is_none() {
                self.confirming_queue = false;
            }
            return;
        };
        self.confirming_queue = false;
        // Two rows writing one file would have the second render over the
        // first one's work.
        if self.queue.iter().any(|item| {
            item.output == output && matches!(item.state, QueueState::Waiting | QueueState::Running)
        }) {
            self.phase = ExportPhase::Failed;
            self.message = t("export.alreadyInQueue");
            return;
        }
        self.phase = ExportPhase::Idle;
        self.message.clear();
        let id = self.next_queue_id;
        self.next_queue_id += 1;
        self.queue.push(QueueItem {
            id,
            project: studio.project_name.clone(),
            output,
            request: Some(request),
            state: QueueState::Waiting,
            progress: 0.0,
        });
        let place = self
            .queue
            .iter()
            .filter(|item| matches!(item.state, QueueState::Waiting | QueueState::Running))
            .count();
        studio.notify(&tf("export.addedToQueue", &[&place]), false);
    }

    /// Starts the first waiting render when nothing holds the exporter.
    fn pump(&mut self, studio: &mut Studio) {
        if self.queue.iter().any(|item| item.state == QueueState::Running) {
            return;
        }
        let Some(index) = self
            .queue
            .iter()
            .position(|item| item.state == QueueState::Waiting)
        else {
            return;
        };
        // An export now, or a stopped render still unwinding: this is asked
        // again when that one's end comes back.
        let Ok(job) = studio.host.exporter.begin() else {
            return;
        };
        let item = &mut self.queue[index];
        let Some(request) = item.request.take() else {
            item.state = QueueState::Failed(t("export.nothingTimelineExport"));
            return;
        };
        item.state = QueueState::Running;
        item.progress = 0.0;
        let id = item.id;
        let compositor = studio.host.monitor.off_thread();
        let low = studio.prefs.export_options.low_priority;
        spawn(
            move || {
                let job = job;
                if low {
                    lower_thread_priority();
                }
                export::run_on(&request, compositor, job.cancel_flag(), |progress| {
                    let fraction = if progress.total > 0 {
                        progress.frame as f32 / progress.total as f32
                    } else {
                        0.0
                    };
                    on_ui(move |studio, _, _| {
                        studio.handle(Msg::Export(ExportMsg::QueueProgress { id, fraction }));
                    });
                })
            },
            move |studio, _, _, result| {
                studio.handle(Msg::Export(ExportMsg::QueueFinished { id, result }));
            },
        );
    }

    /// The custom size is picked: the rung one past the ladder.
    fn is_custom(&self, studio: &Studio) -> bool {
        self.resolution >= Self::ladder(studio).len()
    }

    /// The container's extension for the format picked.
    fn extension(format: usize) -> &'static str {
        if format == 1 { "mov" } else { "mp4" }
    }

    /// One of the file and render options, from the sheet: remembered in
    /// the preferences, except the frame size and the target, which are the
    /// export's.
    fn set_option(&mut self, studio: &mut Studio, key: &str, value: &str) {
        let on = value == "true";
        let number = || value.chars().filter(|c| c.is_ascii_digit()).collect::<String>();
        let options = &mut studio.prefs.export_options;
        match key {
            "format" => options.format = value.parse::<usize>().unwrap_or(0).min(1),
            "audio" => {
                let index = value.parse::<usize>().unwrap_or(1).min(AUDIO_RATES.len() - 1);
                options.audio_kbps = AUDIO_RATES[index];
            }
            "auto-rename" => options.auto_rename = on,
            "append-tag" => options.append_tag = on,
            "low-priority" => options.low_priority = on,
            "notify-sound" => options.notify_sound = on,
            "open-folder" => options.open_folder = on,
            "custom-width" => {
                if let Ok(width) = number().parse::<u32>() {
                    self.custom_width = width.clamp(16, 8192);
                }
                return;
            }
            "custom-height" => {
                if let Ok(height) = number().parse::<u32>() {
                    self.custom_height = height.clamp(16, 8192);
                }
                return;
            }
            "target-mb" => {
                self.target_mb = value.trim().to_owned();
                return;
            }
            // The target size made a bitrate: what the video may spend once
            // the sound has had its share, over the whole timeline - CBR, so
            // the file comes out at the size asked for.
            "target-apply" => {
                let megabytes: f32 = self.target_mb.replace(',', ".").parse().unwrap_or(0.0);
                let seconds = studio.duration().max(1.0);
                if megabytes > 0.0 {
                    let total_kbps = megabytes * 8.0 * 1000.0 / seconds;
                    let video = (total_kbps - studio.prefs.export_options.audio_kbps as f32).max(100.0);
                    self.advanced = true;
                    self.rate_mode = 1;
                    self.bitrate = (video.round() as u32).clamp(100, 200_000);
                    studio.notify(&tf("export.targetApplied", &[&self.bitrate]), false);
                }
                return;
            }
            _ => return,
        }
        studio.prefs.save(&studio.host.dirs);
    }

    /// The queue holds the exporter or will: a render running or waiting.
    fn queue_busy(&self) -> bool {
        self.queue
            .iter()
            .any(|item| matches!(item.state, QueueState::Waiting | QueueState::Running))
    }

    /// "Render 2/5 · 42%" for the title strip while the queue works;
    /// empty when it has nothing to do.
    pub fn queue_status(&self) -> String {
        if !self.queue_busy() {
            return String::new();
        }
        let finished = self
            .queue
            .iter()
            .filter(|item| !matches!(item.state, QueueState::Waiting | QueueState::Running))
            .count();
        let running = self
            .queue
            .iter()
            .find(|item| item.state == QueueState::Running)
            .map_or(0.0, |item| item.progress);
        tf(
            "export.queueStatus",
            &[
                &(finished + 1),
                &self.queue.len(),
                &((running * 100.0).round() as i32),
            ],
        )
    }

    /// The queue's rows as the sheet lists them.
    fn queue_rows(&self) -> Vec<QueueRow> {
        self.queue
            .iter()
            .map(|item| QueueRow {
                name: file_stem(&item.output).into(),
                detail: format!("{} · {}", item.project, item.output).into(),
                state: match item.state {
                    QueueState::Waiting => 0,
                    QueueState::Running => 1,
                    QueueState::Done => 2,
                    QueueState::Failed(_) => 3,
                    QueueState::Cancelled => 4,
                },
                status: match &item.state {
                    QueueState::Waiting => t("export.queueWaiting"),
                    QueueState::Running => format!("{}%", (item.progress * 100.0).round() as i32),
                    QueueState::Done => t("export.queueDone"),
                    QueueState::Failed(error) => tf("export.queueFailed", &[error]),
                    QueueState::Cancelled => t("export.queueCancelled"),
                }
                .into(),
                progress: item.progress,
            })
            .collect()
    }

    /// Checks the sheet and the project and builds the render's request:
    /// the file it writes and the project as it stands, or None with the
    /// sheet saying why - or asking about a file already there.
    fn prepare(
        &mut self,
        studio: &mut Studio,
        overwrite: bool,
    ) -> Option<(String, pamedit_export::ExportRequest)> {
        if studio.session.is_none() {
            return None;
        }
        if studio.timeline().clips.is_empty() {
            self.phase = ExportPhase::Failed;
            self.message = t("export.nothingTimelineExport");
            return None;
        }
        // The name is a file's, or the sheet says what is wrong with it: an
        // empty one wrote a hidden `.mp4`, and a slash made folders (audit
        // 2026-10-09, finding 7).
        let Some(name) = file_name(&self.name) else {
            self.phase = ExportPhase::Failed;
            self.message = t("export.nameNeedsToBeAFileName");
            return None;
        };
        let options = studio.prefs.export_options.clone();
        let extension = Self::extension(options.format);
        let (width, height) = self.size(studio);
        let name = if options.append_tag {
            format!("{name}_{width}x{height}_{}", today())
        } else {
            name
        };
        let folder = self.folder.trim_end_matches('/').to_owned();
        let taken = |path: &str| {
            std::path::Path::new(path).exists()
                || self.queue.iter().any(|item| {
                    item.output == path
                        && matches!(item.state, QueueState::Waiting | QueueState::Running)
                })
        };
        let mut output = format!("{folder}/{name}.{extension}");
        // PAM_Edit Video: a name already taken - on disk or by the queue -
        // becomes Name_01, Name_02... rather than a question.
        if options.auto_rename && !overwrite {
            let mut n = 1;
            while taken(&output) && n < 1000 {
                output = format!("{folder}/{name}_{n:02}.{extension}");
                n += 1;
            }
        }
        // A file already there is replaced once the person has said so:
        // the confirmation sheet asks, and its answer comes back as
        // `Overwrite`. Nothing is written until then; the engine renders
        // beside the file and renames over it at the end.
        if !overwrite && std::path::Path::new(&output).exists() {
            studio.confirm = Some(crate::studio::Confirm::ReplaceExport { path: output });
            return None;
        }
        let session = studio.session.as_ref()?;
        let spec = ExportSpec {
            output: output.clone(),
            crf: EXPORT_CRF[self.quality.min(2)],
            preset: "veryfast".into(),
            codec: self.codec(studio),
            ten_bit: self.deep(studio),
            rate_mode: if self.advanced && self.rate_mode == 1 {
                pamedit_media::RateMode::Cbr
            } else {
                pamedit_media::RateMode::Vbr
            },
            bitrate_kbps: if self.advanced && self.rate_mode == 1 {
                self.bitrate
            } else {
                0
            },
            color_range: self.color_range(),
            hdr: self.hdr(studio),
            gpu_encode: self.gpu_encode,
            audio_kbps: options.audio_kbps,
            // Low priority: half the cores for the encoder, so the window
            // keeps the rest.
            encoder_threads: if options.low_priority {
                (std::thread::available_parallelism().map_or(4, |n| n.get()) / 2).max(2) as u16
            } else {
                0
            },
        };
        let (frame_w, frame_h) = studio.output_size();
        let titles = studio
            .host
            .titles
            .clips(session.project(), frame_w, frame_h)
            .into_iter()
            .map(|title| title.clip)
            .collect();
        let mut request = export::request(session, &spec, titles);
        let (width, height) = self.size(studio);
        let (num, den) = self.rate(studio);
        request.width = width;
        request.height = height;
        request.rate_num = num;
        request.rate_den = den;
        Some((output, request))
    }

    /// The sheet as Slint shows it.
    pub fn data(&self, studio: &Studio) -> ExportData {
        let (width, height) = self.size(studio);
        let (num, den) = self.rate(studio);
        let rate = num as f32 / den as f32;
        let clips = studio.timeline().clips.len();
        let titles = studio
            .timeline()
            .clips
            .iter()
            .filter(|clip| clip.kind == pamedit_project::model::ClipKind::Text)
            .count();
        ExportData {
            queue: ModelRc::new(VecModel::from(self.queue_rows())),
            queue_busy: self.queue_busy(),
            gpu_encode: self.gpu_encode,
            gpu_encode_available: !cfg!(any(target_os = "android", target_os = "ios")),
            open: self.open,
            name: self.name.as_str().into(),
            // Where the file ends up: on a phone, the folder its gallery
            // shows, not the app's own that the export writes into.
            // The name as it will be written: trimmed, as `begin` trims it.
            path: match platform::published_folder() {
                Some(folder) => format!(
                    "{folder}/{}.{}",
                    self.name.trim(),
                    Self::extension(studio.prefs.export_options.format)
                ),
                None => format!(
                    "{}/{}.{}",
                    self.folder.trim_end_matches('/'),
                    self.name.trim(),
                    Self::extension(studio.prefs.export_options.format)
                ),
            }
            .into(),
            format: format!("{width} × {height} · {rate:.2} fps").into(),
            duration: {
                let whole = studio.duration().max(0.0) as i32;
                format!("{}:{:02}", whole / 60, whole % 60).into()
            },
            contents: if titles > 0 {
                format!("{clips} clips · {titles} titles")
            } else {
                format!("{clips} clips")
            }
            .into(),
            resolution: self.resolution as i32,
            resolutions: {
                let sizes: Vec<SharedString> = Self::ladder(studio)
                    .iter()
                    .map(|rung| {
                        let (width, height) = Self::frame_for(studio, *rung);
                        format!("{width} × {height}").into()
                    })
                    .chain(std::iter::once(t("export.customSize").into()))
                    .collect();
                ModelRc::new(VecModel::from(sizes))
            },
            resolution_details: {
                let names: Vec<SharedString> = Self::ladder(studio)
                    .iter()
                    .map(|rung| match rung {
                        2160 => "4K".into(),
                        1440 => "QHD".into(),
                        1080 => "1080p".into(),
                        720 => "720p".into(),
                        _ => t("export.timelineSize").into(),
                    })
                    .chain(std::iter::once(SharedString::new()))
                    .collect();
                ModelRc::new(VecModel::from(names))
            },
            resolution_disabled: {
                let off: Vec<bool> = vec![false; Self::ladder(studio).len() + 1];
                ModelRc::new(VecModel::from(off))
            },
            custom_size: self.is_custom(studio),
            custom_width: self.custom_width.to_string().into(),
            custom_height: self.custom_height.to_string().into(),
            container: studio.prefs.export_options.format as i32,
            audio_index: AUDIO_RATES
                .iter()
                .position(|kbps| *kbps == studio.prefs.export_options.audio_kbps)
                .unwrap_or(1) as i32,
            target_mb: self.target_mb.as_str().into(),
            auto_rename: studio.prefs.export_options.auto_rename,
            append_tag: studio.prefs.export_options.append_tag,
            low_priority: studio.prefs.export_options.low_priority,
            notify_sound: studio.prefs.export_options.notify_sound,
            open_folder: studio.prefs.export_options.open_folder,
            rate: self.rate as i32,
            rates: {
                let names: Vec<SharedString> = Self::rates(studio)
                    .into_iter()
                    .map(|rate| Self::rate_name(rate).into())
                    .collect();
                ModelRc::new(VecModel::from(names))
            },
            rate_details: {
                let mut details: Vec<SharedString> =
                    vec![SharedString::new(); Self::rates(studio).len()];
                details[0] = t("export.timelineSize").into();
                ModelRc::new(VecModel::from(details))
            },
            quality: self.quality as i32,
            codec: pamedit_media::VideoCodec::ALL
                .iter()
                .position(|codec| *codec == self.codec(studio))
                .unwrap_or(0) as i32,
            h264_encodable: pamedit_media::VideoCodec::H264.encodable(),
            hevc_encodable: pamedit_media::VideoCodec::Hevc.encodable(),
            av1_encodable: pamedit_media::VideoCodec::Av1.encodable(),
            ten_bit: self.ten_bit,
            hdr_timeline: studio.timeline().video.color_space.is_hdr(),
            hdr: self.hdr(studio),
            deep_available: pamedit_media::VideoCodec::ALL
                .iter()
                .any(|codec| codec.ten_bit_available()),
            hdr_label: match studio.timeline().video.color_space {
                pamedit_project::model::ColorSpace::Pq => "HDR (PQ)",
                _ => "HDR (HLG)",
            }
            .into(),
            color_range: self.color_range as i32,
            advanced: self.advanced,
            rate_mode: self.rate_mode as i32,
            bitrate: self.bitrate as i32,
            encoding: {
                // "HEVC 10-bit · hardware": the standard, the depth when it
                // is the deeper one, and whether the platform's own encoder
                // will be doing it.
                let codec = self.codec(studio);
                let mut words = vec![codec.label().to_owned()];
                if self.deep(studio) {
                    words.push("10-bit".to_owned());
                }
                if self.hdr(studio) {
                    words.push(
                        match studio.timeline().video.color_space {
                            pamedit_project::model::ColorSpace::Pq => "PQ",
                            _ => "HLG",
                        }
                        .to_owned(),
                    );
                }
                if self.color_range() == ColorRange::Full {
                    words.push(t("export.fullRange"));
                }
                if codec.hardware_encoded_preferring(self.deep(studio), self.gpu_encode) {
                    words.push(format!("· {}", t("export.hardware")));
                }
                words.join(" ")
            }
            .into(),
            size_high: bytes(self.size_bytes(studio, 0)).into(),
            size_balanced: bytes(self.size_bytes(studio, 1)).into(),
            size_small: bytes(self.size_bytes(studio, 2)).into(),
            phase: self.phase,
            progress: self.progress,
            stage: self.stage.as_str().into(),
            eta: if self.phase == ExportPhase::Running && self.progress > 0.02 {
                self.started_at
                    .map(|started| {
                        let elapsed = started.elapsed().as_secs_f32();
                        eta(elapsed / self.progress * (1.0 - self.progress)).into()
                    })
                    .unwrap_or_default()
            } else {
                slint::SharedString::new()
            },
            message: self.message.as_str().into(),
            done_size: bytes(self.size_bytes(studio, self.quality)).into(),
            empty: clips == 0,
        }
    }
}

/// The typed name as the file's, or `None` for one that cannot be: empty
/// once trimmed, a dot or two, or holding a character no file system
/// here takes. The set is Windows' - a slash and a backslash, the colon,
/// the quote and the wildcards, the angle brackets and the bar - on every
/// platform, so a project's export is named the same wherever it is
/// opened; a control character is refused with them.
fn file_name(typed: &str) -> Option<String> {
    let name = typed.trim();
    if name.is_empty() || name.chars().all(|c| c == '.') {
        return None;
    }
    let refused = |c: char| c.is_control() || r#"/\:*?"<>|"#.contains(c);
    (!name.chars().any(refused)).then(|| name.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;

    /// A name is a file's or the export does not start: trimmed, never
    /// empty, never a path, and none of the characters Windows refuses.
    #[test]
    fn the_file_name_is_a_file_name() {
        assert_eq!(file_name("  My cut "), Some("My cut".to_owned()));
        assert_eq!(file_name("cut.final"), Some("cut.final".to_owned()));
        assert_eq!(file_name("Ünïcödé 日本"), Some("Ünïcödé 日本".to_owned()));
        for bad in [
            "", "   ", ".", "..", "a/b", r"a\b", "c:d", "a?b", "<a>", "a|b", "a\"b", "a*", "a\tb",
        ] {
            assert_eq!(file_name(bad), None, "{bad:?}");
        }
    }

    /// The timeline's rate leads the list and is not offered twice; the
    /// NTSC fractions keep their camera names.
    #[test]
    fn the_timeline_rate_comes_first() {
        let rates = ExportPane::rates_around((25, 1));
        assert_eq!(rates[0], (25, 1));
        assert_eq!(rates.iter().filter(|rate| **rate == (25, 1)).count(), 1);
        assert_eq!(rates.len(), RATES.len());
        let rates = ExportPane::rates_around((50, 2));
        assert_eq!(rates.len(), RATES.len(), "25 written as 50/2 is still 25");
        let odd = ExportPane::rates_around((15, 1));
        assert_eq!((odd[0], odd.len()), ((15, 1), RATES.len() + 1));
        assert_eq!(ExportPane::rate_name((24000, 1001)), "23.976 fps");
        assert_eq!(ExportPane::rate_name((25, 1)), "25 fps");
        assert_eq!(ExportPane::rate_name((15, 1)), "15 fps");
        assert_eq!(ExportPane::rate_name((12500, 1000)), "12.5 fps");
    }
}

/// The file name without its folder or extension: what a queue row and its
/// toasts call a render.
fn file_stem(path: &str) -> String {
    std::path::Path::new(path)
        .file_stem()
        .map(|stem| stem.to_string_lossy().into_owned())
        .unwrap_or_else(|| path.to_owned())
}

/// The sheet's audio bitrates, in kbps.
const AUDIO_RATES: [u32; 4] = [128, 192, 256, 320];

/// What a finished render does beyond saying so: a sound and the folder
/// opened on the file, each when the options ask.
fn when_rendered(studio: &mut Studio, written: &str) {
    let options = studio.prefs.export_options.clone();
    if options.notify_sound {
        beep();
    }
    if options.open_folder && !written.is_empty() {
        let _ = platform::reveal(written);
    }
}

/// Today, as the file name's tag: 20261015. The machine's own day, not UTC.
fn today() -> String {
    #[cfg(windows)]
    {
        #[repr(C)]
        #[derive(Default)]
        struct SystemTime {
            year: u16,
            month: u16,
            weekday: u16,
            day: u16,
            hour: u16,
            minute: u16,
            second: u16,
            millis: u16,
        }
        unsafe extern "system" {
            fn GetLocalTime(time: *mut SystemTime);
        }
        let mut now = SystemTime::default();
        unsafe { GetLocalTime(&mut now) };
        format!("{:04}{:02}{:02}", now.year, now.month, now.day)
    }
    #[cfg(not(windows))]
    {
        let days = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |since| since.as_secs() / 86_400) as i64;
        // Days since 1970 to a civil date (Howard Hinnant's algorithm).
        let z = days + 719_468;
        let era = z.div_euclid(146_097);
        let doe = z - era * 146_097;
        let yoe = (doe - doe / 1460 + doe / 36_524 - doe / 146_096) / 365;
        let doy = doe - (365 * yoe + yoe / 4 - yoe / 100);
        let mp = (5 * doy + 2) / 153;
        let day = doy - (153 * mp + 2) / 5 + 1;
        let month = if mp < 10 { mp + 3 } else { mp - 9 };
        let year = yoe + era * 400 + i64::from(month <= 2);
        format!("{year:04}{month:02}{day:02}")
    }
}

/// The render thread below the window's priority, so editing stays smooth.
fn lower_thread_priority() {
    #[cfg(windows)]
    {
        unsafe extern "system" {
            fn GetCurrentThread() -> *mut std::ffi::c_void;
            fn SetThreadPriority(thread: *mut std::ffi::c_void, priority: i32) -> i32;
        }
        const THREAD_PRIORITY_BELOW_NORMAL: i32 = -1;
        unsafe {
            SetThreadPriority(GetCurrentThread(), THREAD_PRIORITY_BELOW_NORMAL);
        }
    }
}

/// The system's notification sound.
fn beep() {
    #[cfg(windows)]
    {
        #[link(name = "user32")]
        unsafe extern "system" {
            fn MessageBeep(kind: u32) -> i32;
        }
        const MB_ICONASTERISK: u32 = 0x40;
        unsafe {
            MessageBeep(MB_ICONASTERISK);
        }
    }
}
