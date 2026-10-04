//! V-Archive 자동 업로드 스케줄러.
//!
//! 안정(stable) 확정된 기록 값이 `delay` 동안 변하지 않으면 업로드 대상으로 내보낸다.
//! 값(곡/모드/난이도/Rate/MAX COMBO)이 바뀌거나 안정 상태가 깨지면 타이머를 처음부터 다시 센다.
//! 결과창에서 대기 중이던 값은 결과창을 벗어나는 즉시 내보내고(flush), 선곡창에서는 취소한다.
//!
//! 시간은 호출자가 주입하므로 egui/네트워크 없이 단위 테스트할 수 있다.

use overmax_core::{PlayContext, SceneType};
use overmax_data::{AutoUploadScope, VArchiveAutoUploadSettings};
use std::time::{Duration, Instant};

/// 한 프레임의 관측값.
pub(crate) struct AutoUploadObservation<'a> {
    pub scene: SceneType,
    /// 안정 확정된 컨텍스트 (`is_stable` 이 아니면 `None`).
    pub stable_context: Option<&'a PlayContext>,
    /// 현재 컨텍스트가 V-Archive 기록보다 낫고 계정이 연결되어 업로드 가능한지.
    pub eligible: bool,
}

struct Pending {
    snapshot: PlayContext,
    is_result: bool,
    since: Instant,
    delay: Duration,
}

#[derive(Default)]
pub(crate) struct AutoUploadScheduler {
    pending: Option<Pending>,
    /// 마지막으로 내보낸 값. 실패 시 같은 값을 매 프레임 재시도하지 않도록 막는다.
    /// 다른 안정 값이 관측되면 해제되어, 선곡창에서 커서를 옮겼다 돌아오면 재시도된다.
    last_fired: Option<PlayContext>,
}

impl AutoUploadScheduler {
    /// 관측값을 반영하고, 업로드할 값이 있으면 반환한다.
    pub(crate) fn tick(
        &mut self,
        now: Instant,
        obs: &AutoUploadObservation<'_>,
        settings: &VArchiveAutoUploadSettings,
    ) -> Option<PlayContext> {
        if !settings.enabled {
            self.pending = None;
            return None;
        }

        let is_result = obs.scene.is_result();
        let in_scope = is_result
            || (settings.scope == AutoUploadScope::SelectAndResult && obs.scene.is_record_scene());

        if let Some(ctx) = obs.stable_context {
            if self.last_fired.as_ref().is_some_and(|last| last != ctx) {
                self.last_fired = None;
            }
        }

        let candidate = obs
            .stable_context
            .filter(|ctx| in_scope && obs.eligible && self.last_fired.as_ref() != Some(*ctx));

        // 결과창을 벗어났으면 대기 중이던 결과창 값을 즉시 내보낸다.
        if !is_result && self.pending.as_ref().is_some_and(|p| p.is_result) {
            return self.pending.take().map(|p| self.fire(p.snapshot));
        }

        match (candidate, &self.pending) {
            (Some(ctx), Some(p)) if p.snapshot == *ctx => {}
            (Some(ctx), _) => {
                self.pending = Some(Pending {
                    snapshot: ctx.clone(),
                    is_result,
                    since: now,
                    delay: Duration::from_secs(settings.delay_sec),
                });
            }
            (None, _) => {
                self.pending = None;
                return None;
            }
        }

        let due = self
            .pending
            .as_ref()
            .is_some_and(|p| now.duration_since(p.since) >= p.delay);
        if due {
            return self.pending.take().map(|p| self.fire(p.snapshot));
        }
        None
    }

    /// 대기 진행률 (0.0..=1.0). 대기 중이 아니면 `None`.
    pub(crate) fn progress(&self, now: Instant) -> Option<f32> {
        let p = self.pending.as_ref()?;
        if p.delay.is_zero() {
            return Some(1.0);
        }
        let ratio = now.duration_since(p.since).as_secs_f32() / p.delay.as_secs_f32();
        Some(ratio.clamp(0.0, 1.0))
    }

    pub(crate) fn is_pending(&self) -> bool {
        self.pending.is_some()
    }

    fn fire(&mut self, snapshot: PlayContext) -> PlayContext {
        self.last_fired = Some(snapshot.clone());
        snapshot
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use overmax_core::{Difficulty, Mode};

    fn ctx(song_id: i32, rate: f32, mc: bool) -> PlayContext {
        PlayContext {
            song_id,
            mode: Mode::B4,
            diff: Difficulty::NM,
            rate,
            is_max_combo: mc,
        }
    }

    fn settings(scope: AutoUploadScope, delay_sec: u64) -> VArchiveAutoUploadSettings {
        VArchiveAutoUploadSettings {
            enabled: true,
            scope,
            delay_sec,
        }
    }

    fn obs(scene: SceneType, c: Option<&PlayContext>) -> AutoUploadObservation<'_> {
        AutoUploadObservation {
            scene,
            stable_context: c,
            eligible: true,
        }
    }

    const RESULT: SceneType = SceneType::ResultFreestyle;
    const SELECT: SceneType = SceneType::Freestyle;

    #[test]
    fn fires_after_value_stays_unchanged_for_delay() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::ResultOnly, 3);
        let t0 = Instant::now();
        let c = ctx(1, 98.5, false);

        assert_eq!(s.tick(t0, &obs(RESULT, Some(&c)), &cfg), None);
        assert_eq!(
            s.tick(
                t0 + Duration::from_millis(2900),
                &obs(RESULT, Some(&c)),
                &cfg
            ),
            None
        );
        assert_eq!(
            s.tick(t0 + Duration::from_secs(3), &obs(RESULT, Some(&c)), &cfg),
            Some(c.clone())
        );
        // 같은 값은 다시 내보내지 않는다.
        assert_eq!(
            s.tick(t0 + Duration::from_secs(10), &obs(RESULT, Some(&c)), &cfg),
            None
        );
    }

    #[test]
    fn rate_or_max_combo_change_restarts_timer() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::ResultOnly, 3);
        let t0 = Instant::now();

        s.tick(t0, &obs(RESULT, Some(&ctx(1, 97.0, false))), &cfg);
        s.tick(
            t0 + Duration::from_secs(2),
            &obs(RESULT, Some(&ctx(1, 98.0, false))),
            &cfg,
        );
        let mc = ctx(1, 98.0, true);
        s.tick(t0 + Duration::from_secs(4), &obs(RESULT, Some(&mc)), &cfg);
        assert_eq!(
            s.tick(
                t0 + Duration::from_millis(6900),
                &obs(RESULT, Some(&mc)),
                &cfg
            ),
            None
        );
        assert_eq!(
            s.tick(t0 + Duration::from_secs(7), &obs(RESULT, Some(&mc)), &cfg),
            Some(mc)
        );
    }

    #[test]
    fn unstable_frame_restarts_timer() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::ResultOnly, 3);
        let t0 = Instant::now();
        let c = ctx(1, 98.5, false);

        s.tick(t0, &obs(RESULT, Some(&c)), &cfg);
        s.tick(t0 + Duration::from_secs(2), &obs(RESULT, None), &cfg);
        s.tick(t0 + Duration::from_secs(3), &obs(RESULT, Some(&c)), &cfg);
        assert_eq!(
            s.tick(t0 + Duration::from_secs(5), &obs(RESULT, Some(&c)), &cfg),
            None
        );
        assert_eq!(
            s.tick(t0 + Duration::from_secs(6), &obs(RESULT, Some(&c)), &cfg),
            Some(c)
        );
    }

    #[test]
    fn leaving_result_screen_flushes_pending_value() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::ResultOnly, 3);
        let t0 = Instant::now();
        let c = ctx(1, 98.5, true);

        s.tick(t0, &obs(RESULT, Some(&c)), &cfg);
        assert_eq!(
            s.tick(
                t0 + Duration::from_secs(1),
                &obs(SceneType::Unknown, None),
                &cfg
            ),
            Some(c)
        );
    }

    #[test]
    fn moving_cursor_on_song_select_cancels_and_returning_retries() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::SelectAndResult, 3);
        let t0 = Instant::now();
        let a = ctx(1, 99.0, false);
        let b = ctx(2, 95.0, false);

        s.tick(t0, &obs(SELECT, Some(&a)), &cfg);
        s.tick(t0 + Duration::from_secs(2), &obs(SELECT, Some(&b)), &cfg);
        assert_eq!(
            s.tick(t0 + Duration::from_secs(3), &obs(SELECT, Some(&b)), &cfg),
            None
        );
        assert_eq!(
            s.tick(t0 + Duration::from_secs(5), &obs(SELECT, Some(&b)), &cfg),
            Some(b.clone())
        );

        // b 업로드가 실패해 여전히 eligible 이라도 같은 값은 재시도하지 않는다.
        assert_eq!(
            s.tick(t0 + Duration::from_secs(9), &obs(SELECT, Some(&b)), &cfg),
            None
        );
        // 다른 곡으로 갔다가 돌아오면 다시 대기한다.
        s.tick(t0 + Duration::from_secs(10), &obs(SELECT, Some(&a)), &cfg);
        s.tick(t0 + Duration::from_secs(11), &obs(SELECT, Some(&b)), &cfg);
        assert_eq!(
            s.tick(t0 + Duration::from_secs(14), &obs(SELECT, Some(&b)), &cfg),
            Some(b)
        );
    }

    #[test]
    fn result_only_scope_ignores_song_select() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::ResultOnly, 0);
        let c = ctx(1, 99.0, false);

        assert_eq!(s.tick(Instant::now(), &obs(SELECT, Some(&c)), &cfg), None);
        assert!(!s.is_pending());
    }

    #[test]
    fn zero_delay_fires_immediately() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::ResultOnly, 0);
        let c = ctx(1, 99.0, false);

        assert_eq!(
            s.tick(Instant::now(), &obs(RESULT, Some(&c)), &cfg),
            Some(c)
        );
    }

    #[test]
    fn ineligible_or_disabled_does_not_fire() {
        let mut s = AutoUploadScheduler::default();
        let t0 = Instant::now();
        let c = ctx(1, 99.0, false);
        let not_eligible = AutoUploadObservation {
            scene: RESULT,
            stable_context: Some(&c),
            eligible: false,
        };
        let cfg = settings(AutoUploadScope::ResultOnly, 0);
        assert_eq!(s.tick(t0, &not_eligible, &cfg), None);

        let disabled = VArchiveAutoUploadSettings::default();
        assert_eq!(s.tick(t0, &obs(RESULT, Some(&c)), &disabled), None);
    }

    #[test]
    fn progress_reports_fill_ratio() {
        let mut s = AutoUploadScheduler::default();
        let cfg = settings(AutoUploadScope::ResultOnly, 3);
        let t0 = Instant::now();
        let c = ctx(1, 98.0, false);

        assert_eq!(s.progress(t0), None);
        s.tick(t0, &obs(RESULT, Some(&c)), &cfg);
        let p = s.progress(t0 + Duration::from_millis(1500)).unwrap();
        assert!((p - 0.5).abs() < 1e-3);
        assert_eq!(s.progress(t0 + Duration::from_secs(9)), Some(1.0));
    }
}
