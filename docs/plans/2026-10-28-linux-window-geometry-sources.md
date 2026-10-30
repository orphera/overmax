# Linux 게임 창 위치 소스 — 환경별 조사와 계약 (TASKS 11.2 · 12.2)

> 상태: 설계 기준 문서. 컴포지터별 내용은 공개 문서·IPC 사양 기준의 사전 조사이며 실환경 검증 전이다. 검증 결과는 12.2 진행 시 이 문서의 "검증" 열에 기록한다.
> 근거: [orphera/overmax#35](https://github.com/orphera/overmax/issues/35) — 창 모드에서 게임 창 이동·크기 변경 시 오버레이가 따라가지 않음
> 관련: [2026-10-28-app-boundary-inventory.md](2026-10-28-app-boundary-inventory.md), [linux_support.md](../decisions/linux_support.md), 지원 범위는 [docs/guides/linux-support.md](../guides/linux-support.md)

---

## 1. 문제의 뿌리 — native Wayland 제약

Wayland는 설계상 일반 클라이언트에게 다른 창의 전역 좌표를 주지 않는다.

| 경로 | 주는 것 | 못 주는 것 |
|:---|:---|:---|
| `wlr-layer-shell` | 출력(모니터)에 고정된 표면 | 특정 창에 붙는 표면 |
| `wlr-foreign-toplevel` | 창 활성·전체화면 상태, 소속 출력 | 창 좌표·크기 |
| XWayland 루트 좌표 (`overmax_engine` X11 트래커) | X11 좌표계의 게임 rect | 컴포지터 논리 좌표와의 일치 보장 (배율·`force_zero_scaling` 등 설정 의존) |

현재 오버레이는 전체화면일 때 출력 영역에 스냅하고, 창 모드면 수동 위치로 돌아간다(`linux_layer_overlay.rs` `uses_manual_position`). 표준 프로토콜로는 해결 경로가 없으므로 **컴포지터별 IPC 어댑터**가 필요하고, 그 자리가 `GameGeometrySource` trait이다.

---

## 2. 환경별 위치 소스

| 환경 | 위치 소스 | 이벤트 | 가능성 | 검증 |
|:---|:---|:---|:---|:---|
| Hyprland | `$HYPRLAND_INSTANCE_SIGNATURE` 소켓 — `clients -j`의 `at`/`size`, socket2 이벤트 | 있음 | 높음 (전역 논리 좌표 직접 제공) | 미검증 |
| Sway / i3 호환 | `$SWAYSOCK` IPC — `get_tree`의 `rect`, `window` 이벤트 구독 | 있음 | 높음 | 미검증 |
| niri | `$NIRI_SOCKET` IPC — 전체 초기 상태를 주는 이벤트 스트림, 창·레이아웃 변경 이벤트 | 있음 | 중간 (`outputs.logical`은 compositor 좌표, 창 위치는 workspace-view 기준 논리 좌표) | IPC 필드는 문서 확인; 좌표 매핑은 미검증 |
| KDE Plasma (KWin) | KWin 스크립팅 API — 공개된 API에서 창 추가·삭제·활성 신호는 확인, 창별 geometry 변경 신호는 미확인 | 부분 확인 | 낮음 (geometry 이벤트 경로 및 설치·버전 유지보수 부담) | 공개 API만으로 구현 가능성 미확정 |
| Wayfire | IPC 플러그인 (사용자가 활성화 필요) | 일부 | 낮음 — 제보 시 검토 | 미검증 |
| river, labwc, Mango 등 | 창 좌표 IPC 없음 | — | 수동 배치 유지 | — |
| GNOME (Mutter) | layer-shell 미지원으로 오버레이 자체 불가 | — | 범위 밖 | — |
| Gamescope / Steam Deck | 별도 XWayland 세션 | — | 범위 밖 | — |
| X11 데스크톱 세션 | 좌표는 정확하나 X11 오버레이 없음 | — | 범위 밖 (2026-08-11 X11 overlay fallback 보류 결정 유지) | — |

구현 우선순위 (12.2): Hyprland·Sway(전역 좌표 직접 제공, 검증 용이) → niri(좌표 매핑 실환경 검증) → KWin(geometry 이벤트 경로 확인 후 재평가).

### niri 및 KWin 조사 (2026-10-30)

- niri의 [IPC 문서](https://github.com/niri-wm/niri/blob/main/docs/wiki/IPC.md)는 event stream이 연결 시 전체 상태를 먼저 제공하고 이후 변경 이벤트를 계속 보내며, `WindowOpenedOrChanged`와 `WindowLayoutsChanged`를 포함한다고 명시한다. 따라서 폴링 없이 위치 변경을 추적할 수 있는 IPC 경로는 있다.
- [WindowLayout](https://niri-wm.github.io/niri/niri_ipc/struct.WindowLayout.html)의 위치·크기는 논리 좌표지만, `tile_pos_in_workspace_view`는 workspace view 기준이고 창 자체의 시각적 위치는 `window_offset_in_tile`을 더해야 한다. [LogicalOutput](https://niri-wm.github.io/niri/niri_ipc/struct.LogicalOutput.html)은 compositor 좌표의 출력 원점과 크기를 제공한다. 이 값을 조합하는 후보 경로는 있으나, workspace view 원점과 출력 원점의 일치 및 floating/fullscreen 창의 필드 제공 여부는 문서만으로 확정하지 못했다. 실제 niri에서 좌표를 비교하기 전까지 어댑터를 구현하거나 지원한다고 표시하지 않는다.
- KWin [스크립팅 API](https://develop.kde.org/docs/plasma/kwin/api/)는 `windowAdded`, `windowRemoved`, `windowActivated`와 workspace 전환 신호를 문서화하지만, 조사한 공개 API에는 창별 geometry 변경 신호가 확인되지 않았다. 이전 표의 `frameGeometryChanged` 경로는 검증된 계약이 아니므로 구현 근거로 사용하지 않는다. [스크립트 설치 문서](https://develop.kde.org/docs/plasma/kwin/)상 KWin 스크립트는 별도 패키징 및 활성화가 필요하며 KPackage 스크립트는 설치 후 추가 작업용 사용자 정의 스크립트를 제공할 수 없다. geometry 이벤트 경로가 입증되기 전까지 유지보수 비용 대비 우선순위를 낮게 둔다.

---

## 3. `GameGeometrySource` 계약

위치: `overmax_engine::capture::window_tracker` (Linux 전용).

1. **X11 창 ID 비의존**: 창은 제목(또는 app_id)으로 찾는다. Proton native Wayland 드라이버로 게임이 XWayland를 벗어나도 계약이 유지되어야 한다.
2. **푸시 방식**: 소스는 자기 스레드에서 IPC 이벤트를 구독하고 공유 슬롯(`SharedGameGeometry`)에 쓴다. 오버레이는 읽기만 한다. 폴링은 인게임 성능 제약상 지양.
3. **반환값**: 전역 논리 좌표의 게임 rect + 출력 이름(`wl_output` name) + generation. 출력 기준 변환은 오버레이가 기존 출력 geometry로 수행한다.
4. **fail-closed**: 다음 경우 소스는 슬롯을 `None`으로 비운다 — 추적 상실, 같은 제목 창 복수 매칭, 게임 창이 보이지 않는 workspace에 있음, IPC 연결 끊김. 오버레이는 값이 없거나 출력 이름이 현재 대상 출력과 다르면 기존 수동 배치를 유지한다.
5. **선택**: 사용자 결정(2026-10-30)에 따라 `HYPRLAND_INSTANCE_SIGNATURE`와 `SWAYSOCK` 중 하나만 설정되어 있으면 해당 소스를 자동 선택한다. 둘 다 설정되었거나 둘 다 없으면 소스 없음(`None`만 유지)으로 fail-closed한다. 현재 Hyprland·Sway 어댑터는 IPC fixture 테스트만 완료했으며 실환경 검증 전이다.

---

## 4. 채택하지 않은 대안

| 대안 | 기각 이유 |
|:---|:---|
| XWayland 좌표를 배율 1일 때만 신뢰 | 컴포지터 설정에 따라 조용히 틀린 위치를 낼 수 있어 fail-closed 원칙 위반 |
| foreign-toplevel 관찰을 engine으로 이동 | 좌표를 주지 않아 #35에 기여하지 않고, 오버레이의 Wayland 연결·`wl_output` 객체 공유 문제만 생김 |
| Windows/Linux `WindowTracker` 전체 trait 통합 | 두 API와 감지 tick 루프가 근본적으로 다르고 현재 요구가 없음 (안정화 코드 게이트) |
| native Wayland 게임 캡처 (PipeWire/portal) | 매 실행 사용자 승인 등 별도 주제. 계약 1번으로 경로만 열어 둠 |
