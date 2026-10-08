# Linux 지원 안내

[English](linux-support.en.md)

Overmax의 Linux 지원은 초기 단계입니다. Windows와 같은 범용 지원이 아니며, 아래 조건을 만족하는 Proton/XWayland 환경만 현재 지원 대상으로 봅니다.

## 지원 범위

| 게임 실행 방식 / 환경              | 지원 상태                       | 사용 시 알아둘 점                                                                                                                                                                                           |
| ---------------------------------- | ------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| 테두리 없는 전체화면               | **지원 · 권장**                 | 아래 필수 환경을 만족해야 합니다.                                                                                                                                                                           |
| 창 모드                            | **사용 가능하나 사실상 미지원** | 단일 모니터에서 캡처·인식이 가능하지만 정상 동작을 보장하지 않습니다. 오버레이를 직접 배치해야 하며, 게임 창을 이동해도 따라가지 않습니다. 고정된 창 크기로 화면 비율이 달라지면 인식에 실패할 수 있습니다. |
| 독점 전체화면                      | 검증 부족                       | 호환성을 보장하지 않습니다. 테두리 없는 전체화면을 사용해 주세요.                                                                                                                                           |
| GNOME (Mutter)                     | 미지원                          | `wlr-layer-shell` 미지원으로 Overmax 오버레이를 사용할 수 없습니다.                                                                                                                                         |
| Gamescope / Steam Deck Gaming Mode | 미지원                          | Gamescope의 별도 XWayland 세션에 대한 창 추적·캡처·오버레이 연동을 현재 지원·검증하지 않습니다.                                                                                                             |

### 필수 환경

- x86_64 Linux와 glibc 2.35 이상
- `wlr-layer-shell`을 지원하는 Wayland compositor
- 같은 세션에서 동작하는 XWayland
- Vulkan 드라이버
- fontconfig와 한글 글꼴
- 같은 `DISPLAY`에서 Proton/XWayland로 실행한 DJMAX RESPECT V

**데스크톱은 Wayland 세션, 게임은 그 세션의 XWayland에서 실행해야 합니다.** X11 데스크톱 세션이나 native Wayland로 실행한 게임은 지원하지 않습니다.

## 검증 환경

- 배포 번들 빌드·라이브러리 호환성 기준: Ubuntu 22.04 / glibc 2.35. Ubuntu 기본 GNOME 데스크톱에서의 오버레이 지원을 뜻하지 않습니다.
- 캡처 lifecycle CI: Xvfb + Openbox. 창 추적·캡처 자동 테스트용이며, X11 데스크톱의 오버레이 지원을 뜻하지 않습니다.

| 데스크톱 환경 / 윈도우 매니저          | 검증 결과                                                                                                   |
| -------------------------------------- | ----------------------------------------------------------------------------------------------------------- |
| KDE Plasma(KWin), Hyprland, niri, Sway | 게임 추적·캡처·오버레이 표시 확인                                                                           |
| MangoWM                                | 오버레이 표시 확인. 0.15.x~0.16.1에서는 [마우스 입력 문제](#mango-layer-shell-입력-회귀)에 주의해야 합니다. |

위 결과는 확인한 환경에서의 동작을 뜻하며, 모든 버전·설정 조합이나 창 모드의 정상 동작을 보장하지 않습니다.

## 내 환경 확인하기

압축을 푼 디렉터리에서 다음 명령을 실행합니다.

```bash
uname -m
getconf GNU_LIBC_VERSION
printf 'session=%s WAYLAND_DISPLAY=%s DISPLAY=%s\n' \
  "${XDG_SESSION_TYPE:-unset}" "${WAYLAND_DISPLAY:-unset}" "${DISPLAY:-unset}"
ldd ./overmax
fc-match ':lang=ko' | head -n 1
```

- `uname -m`: `x86_64`
- `getconf GNU_LIBC_VERSION`: `glibc 2.35` 이상
- 세션: `session=wayland`이며 `WAYLAND_DISPLAY`와 `DISPLAY`가 모두 설정됨
- `ldd`: `not found`인 공유 라이브러리가 없음
- `fc-match`: 사용할 한글 글꼴이 출력됨

## 설치 및 실행

1. Releases에서 `overmax-linux-x86_64.tar.gz`를 받습니다.
2. 사용자 쓰기 권한이 있는 디렉터리에 압축을 풉니다.
3. DJMAX RESPECT V를 Proton/XWayland의 **테두리 없는 전체화면**으로 실행합니다. 창 모드는 사용 가능하지만 사실상 미지원입니다.
4. 같은 데스크톱 세션의 터미널에서 `./overmax`를 실행합니다.

설정과 캐시는 실행 디렉터리에 저장됩니다. 자동 업데이트를 사용하려면 실행 파일이 있는 디렉터리에 쓰기 권한이 있어야 합니다. 직접 업데이트하는 경우 `settings.user.json`과 `cache/`를 함께 복사합니다.

## 터미널 없이 실행

Overmax 설정에서 **System → Linux 앱 실행 → 바로가기 생성**을 누르면 현재 실행 파일과 실행 디렉터리를 사용하는 `overmax.desktop`이 사용자 앱 메뉴에 생성됩니다. 이후 앱 메뉴에서 Overmax를 실행할 수 있습니다.

설치 디렉터리를 옮긴 경우 이전 바로가기는 옛 경로를 가리키므로 새 위치에서 Overmax를 실행한 뒤 바로가기를 다시 생성해 주세요.

## Steam에서 게임과 함께 자동 실행

Steam의 DJMAX RESPECT V **속성 → 일반 → 시작 옵션**에 다음 명령을 입력합니다. (`OVERMAX_DIR`은 압축을 푼 Overmax 디렉터리로 수정)

```bash
sh -c '(cd "OVERMAX_DIR" && exec ./overmax) & exec "$@"' -- %command%
```

## 실행되지 않을 때

터미널에서 `./overmax`를 실행해 처음 표시되는 오류를 확인한 뒤 아래 항목을 적용합니다.

| 증상 또는 확인 결과                                   | 해결 방법                                                                                                                                                         |
| ----------------------------------------------------- | ----------------------------------------------------------------------------------------------------------------------------------------------------------------- |
| `Permission denied`                                   | `chmod +x ./overmax`를 실행합니다.                                                                                                                                |
| `Exec format error` 또는 `uname -m`이 `x86_64`가 아님 | 현재 배포 번들은 x86_64에서만 실행할 수 있습니다.                                                                                                                 |
| `GLIBC_2.35 not found` 또는 glibc 2.35 미만           | glibc 2.35 이상인 배포판에서 실행합니다.                                                                                                                          |
| `ldd`에 `not found`가 표시됨                          | 표시된 공유 라이브러리를 제공하는 배포판 패키지를 설치합니다.                                                                                                     |
| `WAYLAND_DISPLAY is not set`                          | 로그아웃한 뒤 Wayland 세션으로 로그인하고 같은 세션의 터미널에서 실행합니다.                                                                                      |
| `DISPLAY is not set` 또는 `X11 connect failed`        | XWayland를 활성화하고 게임과 Overmax를 같은 데스크톱 세션에서 실행합니다.                                                                                         |
| 오류 없이 바로 종료됨                                 | 이미 실행 중인 Overmax가 있는지 `pgrep -a overmax`로 확인합니다. 없다면 `XDG_RUNTIME_DIR`이 설정된 정상적인 데스크톱 세션에서 다시 실행합니다.                    |
| `zwlr_layer_shell_v1 is unavailable`                  | `wlr-layer-shell`을 지원하는 compositor를 사용합니다.                                                                                                             |
| Vulkan adapter/device/surface 오류                    | GPU 제조사가 제공하는 Vulkan 드라이버와 로더를 설치하거나 갱신합니다.                                                                                             |
| `Composite` 또는 `MIT-SHM` 오류                       | XWayland의 XComposite와 MIT-SHM 확장이 활성화된 세션을 사용합니다. Gamescope 내부 세션은 현재 지원하지 않습니다.                                                  |
| 한글이 보이지 않거나 `fc-match` 실패                  | fontconfig와 한글 글꼴을 설치한 뒤 `fc-cache -f`를 실행합니다.                                                                                                    |
| `DJMAX RESPECT V window not found`                    | 게임을 먼저 실행하고 Proton이 native Wayland가 아닌 XWayland를 사용하며 게임과 Overmax의 `DISPLAY`가 같은지 확인합니다.                                           |
| 게임 창은 찾지만 오버레이가 정상 표시되지 않음        | 게임을 테두리 없는 전체화면으로 바꿉니다.                                                                                                                         |
| 창모드에서 게임 창 이동 후 오버레이가 따라오지 않음   | 오버레이를 직접 다시 배치합니다.                                                                                                                                  |
| 설정 또는 캐시 저장 시 권한 오류                      | bundle을 사용자 쓰기 권한이 있는 디렉터리에 다시 풉니다.                                                                                                          |
| 업데이트 후 실행되지 않음                             | 새 bundle 전체를 다시 풀고 [실행 모드별 백업·복원 안내](#설정기록-저장-위치와-백업)에 따라 데이터를 보존합니다. 이전 실행 파일이나 공유 라이브러리와 섞지 마세요. |

## 알려진 문제

### 타일링 윈도우 매니저에서 고정된 창 크기로 인한 인식 실패

타일링 윈도우 매니저에서 게임을 고정된 창 크기로 실행하면 게임 화면의 비율과 배치가 정상적인 전체화면과 달라져 Overmax가 화면을 인식하지 못할 수 있습니다. 게임 창을 찾거나 캡처하는 데 성공해도 인식은 실패할 수 있습니다.

이 경우 게임을 **테두리 없는 전체화면**으로 전환해 주세요. 창 모드에서 발생하는 화면 비율 문제는 현재 지원 범위에 포함되지 않습니다.

### Mango layer-shell 입력 회귀

Mango 0.16.1에서는 Overmax layer-shell 오버레이가 정상 표시되어도 마우스 입력이 뒤의 게임 창으로 전달될 수 있습니다. Mango 0.15.x부터 layer-shell surface의 마우스·키보드 입력이 무시되는 동일 증상이 [Mango issue #1214](https://github.com/mangowm/mango/issues/1214)에 보고되어 있습니다.

Overmax에서 표시 상태의 input region과 `seat0` pointer binding을 확인했지만, compositor로부터 `wl_pointer.enter`, `motion`, `button` 이벤트가 전달되지 않았습니다. 따라서 이 버전 범위에서는 Overmax 설정이나 입력 영역 변경으로 해결할 수 없습니다.

Mango upstream에서 수정된 버전을 사용하거나, 수정 전까지 정상 동작이 보고된 0.14.x 또는 다른 `wlr-layer-shell` compositor를 사용합니다. 게임의 키보드 포커스를 빼앗을 수 있는 `KeyboardInteractivity` 강제 변경은 우회책으로 사용하지 않습니다.

## 현재 미지원 기능과 환경

- 창 모드의 정상 동작 보장 (사용은 가능하지만 사실상 미지원)
- 창모드에서 게임 창을 따라가는 자동 오버레이 배치
- 다중 출력의 창모드
- GNOME (Mutter): `wlr-layer-shell` 미지원으로 오버레이 사용 불가. [프로토콜 호환성 안내](https://github.com/wmww/gtk-layer-shell#supported-desktops)
- Gamescope 및 Steam Deck Gaming Mode: Gamescope 내부 세션의 창 추적·캡처·오버레이 연동을 현재 지원·검증하지 않음. Gamescope는 [별도의 XWayland 환경](https://github.com/ValveSoftware/gamescope#gamescope-the-micro-compositor-formerly-known-as-steamcompmgr)을 사용하므로, 일반 데스크톱에서의 검증 결과를 그대로 적용할 수 없음
- X11 데스크톱 세션: 현재 오버레이는 Wayland의 `wlr-layer-shell`이 필요하며 X11용 오버레이는 제공하지 않음

## 아직 검증 범위가 부족한 환경

다음 환경은 현재 호환성을 보장할 만큼 검증되지 않았습니다.

- compositor와 배포판별 차이
- GPU 제조사와 드라이버 조합
- 독점 전체화면
- fractional scaling 및 HiDPI 조합
- 여러 Proton 버전
