"""Background scan and row actions for the V-Archive sync window."""

import threading

from data.sync_manager import SyncCandidate, build_candidates
from data.varchive_uploader import AccountInfo, upload_score
from overlay.sync_candidate_row import CandidateRow


_BUTTON_NUM_BY_MODE = {
    "4B": 4,
    "5B": 5,
    "6B": 6,
    "8B": 8,
}


class SyncActionsMixin:
    """SyncWindow mixin that owns worker threads and candidate mutations."""

    def _start_scan(self):
        if self._record_manager is None:
            self._status_label.setText("기록 관리자가 초기화되지 않았습니다.")
            return
        if self._scan_in_progress:
            self._rescan_queued = True
            return

        self._scan_in_progress = True
        self._update_ui_states()
        self._status_label.setText("비교 중...")
        self._clear_list()
        self._empty_label.setText("분석 중...")
        self._empty_label.show()
        threading.Thread(target=self._scan_worker, daemon=True).start()

    def _scan_worker(self):
        try:
            if not self._current_steam_id:
                raise ValueError("steam_id is not set for SyncWindow scan")
            candidates = build_candidates(
                self._vdb,
                self._record_manager,
                self._current_steam_id,
            )
        except Exception as e:
            candidates = []
            print(f"[SyncWindow] 스캔 오류: {e}")
        self._signals.scan_finished.emit(candidates)

    def _on_scan_finished(self, candidates: list[SyncCandidate]):
        self._scan_in_progress = False
        self._candidates = candidates
        self._update_ui_states()
        self._clear_list()

        if not candidates:
            self._show_empty_scan_result()
            return

        self._empty_label.hide()
        has_account = self._get_current_account() is not None
        for index, candidate in enumerate(candidates):
            row = CandidateRow(index, candidate)
            row.set_upload_enabled(has_account)
            row.upload_requested.connect(self._on_upload_requested)
            row.delete_requested.connect(self._on_delete_requested)
            self._list_layout.addWidget(row)
            self._rows.append(row)

        count = len(candidates)
        self._count_label.setText(f"— {count}개 후보")
        self._status_label.setText(f"{count}개의 갱신 후보를 찾았습니다.")
        self.adjustSize()
        self._start_queued_rescan_if_needed()

    def _show_empty_scan_result(self):
        self._empty_label.setText("동기화 후보가 없습니다. V-Archive 기록이 이미 최신입니다.")
        self._empty_label.show()
        self._count_label.setText("")
        self._status_label.setText("최신 상태입니다.")
        self._start_queued_rescan_if_needed()

    def _start_queued_rescan_if_needed(self):
        if not self._rescan_queued:
            return
        self._rescan_queued = False
        self._start_scan()

    def _clear_list(self):
        for i in range(self._list_layout.count()):
            item = self._list_layout.itemAt(i)
            if item and item.widget() and item.widget() != self._empty_label:
                item.widget().deleteLater()
        self._rows = []
        self._empty_label.show()

    def _on_upload_requested(self, index: int):
        account = self._get_current_account()
        if account is None or index >= len(self._candidates):
            return

        if index < len(self._rows):
            self._rows[index].set_status("pending", "")

        threading.Thread(
            target=self._upload_worker,
            args=(index, self._candidates[index], account),
            daemon=True,
        ).start()

    def _on_delete_requested(self, index: int):
        if index >= len(self._candidates):
            return

        if index < len(self._rows):
            self._rows[index].set_status("pending", "")

        threading.Thread(
            target=self._delete_worker,
            args=(index, self._candidates[index]),
            daemon=True,
        ).start()

    def _upload_worker(self, index: int, candidate: SyncCandidate, account: AccountInfo):
        result = upload_score(
            account=account,
            song_name=candidate.song_name,
            button_mode=candidate.button_mode,
            difficulty=candidate.difficulty,
            score=candidate.overmax_rate,
            is_max_combo=candidate.overmax_mc,
            composer=candidate.composer,
        )

        if result.success:
            status = "success" if result.updated else "no_update"
            message = ""
            if result.updated:
                self._update_varchive_cache_after_upload(candidate)
        else:
            status = "error"
            message = result.message

        self._signals.row_status_changed.emit(index, status, message)
        self._signals.action_finished.emit()

    def _update_varchive_cache_after_upload(self, candidate: SyncCandidate):
        button = _BUTTON_NUM_BY_MODE.get(candidate.button_mode)
        if button is None:
            return

        steam_id = self._current_steam_id
        if steam_id == "__unknown__":
            return

        vclient = getattr(self._record_manager, "vclient", None)
        if vclient is None:
            return

        success = vclient.upsert_cached_record(
            steam_id=steam_id,
            button=button,
            song_id=candidate.song_id,
            difficulty=candidate.difficulty,
            score=candidate.overmax_rate,
            is_max_combo=candidate.overmax_mc,
        )
        if success:
            self._record_manager.refresh()

    def _delete_worker(self, index: int, candidate: SyncCandidate):
        success = self._record_manager.delete(
            song_id=candidate.song_id,
            button_mode=candidate.button_mode,
            difficulty=candidate.difficulty,
        )

        status = "success" if success else "error"
        message = "" if success else "삭제 실패"
        self._signals.row_status_changed.emit(index, status, message)
        self._signals.action_finished.emit()

    def _on_row_status(self, index: int, status: str, message: str):
        if index < len(self._rows):
            self._rows[index].set_status(status, message)

    def _on_action_finished(self):
        self._start_scan()
