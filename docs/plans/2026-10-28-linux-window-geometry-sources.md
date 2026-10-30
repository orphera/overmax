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
| niri | `$NIRI_SOCKET` IPC — 창·workspace event stream, 출력·layer 목록은 별도 IPC 조회 | 있음 | 중간 (`outputs.logical`은 compositor 좌표, 창 위치는 workspace-view 기준 논리 좌표) | 문서 기반 adapter 및 fixture 구현; 실환경 좌표 매핑은 미검증 |
| KDE Plasma (KWin) | KWin 스크립팅 API — 공개된 API에서 창 추가·삭제·활성 신호는 확인, 창별 geometry 변경 신호는 미확인 | 부분 확인 | 낮음 (geometry 이벤트 경로 및 설치·버전 유지보수 부담) | 공개 API만으로 구현 가능성 미확정 |
| Wayfire | IPC 플러그인 (사용자가 활성화 필요) | 일부 | 낮음 — 제보 시 검토 | 미검증 |
| river, labwc, Mango 등 | 창 좌표 IPC 없음 | — | 수동 배치 유지 | — |
| GNOME (Mutter) | layer-shell 미지원으로 오버레이 자체 불가 | — | 범위 밖 | — |
| Gamescope / Steam Deck | 별도 XWayland 세션 | — | 범위 밖 | — |
| X11 데스크톱 세션 | 좌표는 정확하나 X11 오버레이 없음 | — | 범위 밖 (2026-08-11 X11 overlay fallback 보류 결정 유지) | — |

구현 우선순위 (12.2): Hyprland·Sway(전역 좌표 직접 제공, 검증 용이) → niri(좌표 매핑 실환경 검증) → KWin(geometry 이벤트 경로 확인 후 재평가).

### niri 및 KWin 조사 (2026-10-30, 2026-10-31 갱신)

- niri의 [IPC 문서](https://github.com/niri-wm/niri/blob/main/docs/wiki/IPC.md)는 연결 시 `WorkspacesChanged`/`WindowsChanged` 초기 상태와 이후 변경 이벤트를 event stream으로 보낸다고 명시한다. `Outputs`와 `Layers`는 별도 요청이므로 창/workspace 관련 이벤트 및 1초 주기로 다시 조회한다. 출력 구성 자체의 변경 이벤트는 없으므로 출력 변경은 최대 약 1초 뒤 반영된다. event stream의 idle read timeout은 재연결로 취급하지 않고, 부분 JSON 라인은 다음 read까지 보존한다.
- [WindowLayout](https://niri-wm.github.io/niri/niri_ipc/struct.WindowLayout.html)의 위치·크기는 논리 좌표다. 창의 시각 rect는 `tile_pos_in_workspace_view + window_offset_in_tile`이며 출력 전역 원점은 [LogicalOutput](https://niri-wm.github.io/niri/niri_ipc/struct.LogicalOutput.html)의 `x/y`를 더한다. niri 소스에서 workspace 위치는 출력 작업영역(`non_exclusive_zone`) 기준으로 계산되지만 IPC `Layers` 응답은 exclusive zone을 제공하지 않는다. 현재 adapter는 `NIRI_SOCKET` 단독 감지, 유일한 제목 일치, active workspace의 active window, 유효한 출력/rect 조건을 요구하며, 대상 출력에 `overmax` 외 layer surface가 있으면 위치를 발행하지 않는다. fractional logical 좌표는 기존 `WindowRect` 정수 계약에 맞춰 반올림한다.
- 2026-10-31 결정: Windows 개발 환경에서 작업을 이어갈 수 있도록 공개 문서와 niri 소스 코드를 근거로 adapter 및 fixture 테스트를 선구현한다. 출력 원점·작업영역 매핑, 배율, floating/fullscreen 및 workspace 이동은 실제 niri에서 확인되지 않았으므로 지원 완료로 표시하지 않는다. 실환경 검증 전에는 external layer surface가 있는 출력을 fail-closed한다.
- KWin [스크립팅 API](https://develop.kde.org/docs/plasma/kwin/api/)는 `windowAdded`, `windowRemoved`, `windowActivated`와 workspace 전환 신호를 문서화하지만, 조사한 공개 API에는 창별 geometry 변경 신호가 확인되지 않았다. 이전 표의 `frameGeometryChanged` 경로는 검증된 계약이 아니므로 구현 근거로 사용하지 않는다. [스크립트 설치 문서](https://develop.kde.org/docs/plasma/kwin/)상 KWin 스크립트는 별도 패키징 및 활성화가 필요하며 KPackage 스크립트는 설치 후 추가 작업용 사용자 정의 스크립트를 제공할 수 없다. geometry 이벤트 경로가 입증되기 전까지 유지보수 비용 대비 우선순위를 낮게 둔다.

---

## 3. `GameGeometrySource` 계약

위치: `overmax_engine::capture::window_tracker` (Linux 전용).

1. **X11 창 ID 비의존**: 창은 제목(또는 app_id)으로 찾는다. Proton native Wayland 드라이버로 게임이 XWayland를 벗어나도 계약이 유지되어야 한다.
2. **푸시 방식**: 소스는 자기 스레드에서 IPC 이벤트를 구독하고 공유 슬롯(`SharedGameGeometry`)에 쓴다. 오버레이는 읽기만 한다. 폴링은 인게임 성능 제약상 지양.
3. **반환값**: 전역 논리 좌표의 게임 rect + 출력 이름(`wl_output` name) + generation. 출력 기준 변환은 오버레이가 기존 출력 geometry로 수행한다.
4. **fail-closed**: 다음 경우 소스는 슬롯을 `None`으로 비운다 — 추적 상실, 같은 제목 창 복수 매칭, 게임 창이 보이지 않는 workspace에 있음, IPC 연결 끊김. 오버레이는 값이 없거나 출력 이름이 현재 대상 출력과 다르면 기존 수동 배치를 유지한다.
5. **선택**: `HYPRLAND_INSTANCE_SIGNATURE`, `SWAYSOCK`, `NIRI_SOCKET` 중 하나만 설정되어 있으면 해당 소스를 자동 선택한다. 둘 이상 설정되거나 모두 없으면 소스 없음(`None`만 유지)으로 fail-closed한다. Hyprland·Sway는 IPC fixture 테스트 완료, Niri는 문서 기반 adapter/fixture 완료이며 세 컴포지터 모두 실환경 검증 전이다.

---

## 4. 채택하지 않은 대안

| 대안 | 기각 이유 |
|:---|:---|
| XWayland 좌표를 배율 1일 때만 신뢰 | 컴포지터 설정에 따라 조용히 틀린 위치를 낼 수 있어 fail-closed 원칙 위반 |
| foreign-toplevel 관찰을 engine으로 이동 | 좌표를 주지 않아 #35에 기여하지 않고, 오버레이의 Wayland 연결·`wl_output` 객체 공유 문제만 생김 |
| Windows/Linux `WindowTracker` 전체 trait 통합 | 두 API와 감지 tick 루프가 근본적으로 다르고 현재 요구가 없음 (안정화 코드 게이트) |
| native Wayland 게임 캡처 (PipeWire/portal) | 매 실행 사용자 승인 등 별도 주제. 계약 1번으로 경로만 열어 둠 |
