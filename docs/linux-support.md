# Linux 지원 안내

Overmax의 Linux 지원은 초기 단계입니다. Windows와 같은 범용 지원이 아니며, 아래 조건을 만족하는 Proton/XWayland 환경만 현재 지원 대상으로 봅니다.

## 지원 범위

다음 조건을 만족해야 합니다.

- x86_64 Linux와 glibc 2.39 이상
- Wayland 세션과 `wlr-layer-shell`을 지원하는 compositor
- 같은 세션에서 동작하는 XWayland
- XComposite 0.2 이상과 MIT-SHM 1.2 이상
- Vulkan 드라이버와 premultiplied transparency를 지원하는 Wayland surface 환경
- fontconfig와 한글 글꼴
- 같은 `DISPLAY`에서 Proton/XWayland로 실행한 DJMAX RESPECT V
- 테두리 없는 전체화면과 단일 출력

공식 Linux 배포 번들은 glibc 2.39 ABI를 기준으로 고정된 CI 환경에서 빌드합니다.

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
- `getconf GNU_LIBC_VERSION`: `glibc 2.39` 이상
- 세션: `session=wayland`이며 `WAYLAND_DISPLAY`와 `DISPLAY`가 모두 설정됨
- `ldd`: `not found`인 공유 라이브러리가 없음
- `fc-match`: 사용할 한글 글꼴이 출력됨

## 설치 및 실행

1. Releases에서 `overmax-linux-x86_64.tar.gz`를 받습니다.
2. 사용자 쓰기 권한이 있는 디렉터리에 압축을 풉니다.
3. DJMAX RESPECT V를 Proton/XWayland의 테두리 없는 전체화면으로 실행합니다.
4. 같은 데스크톱 세션의 터미널에서 `./overmax`를 실행합니다.

설정과 캐시는 실행 디렉터리에 저장됩니다. 자동 업데이트를 사용하려면 실행 파일이 있는 디렉터리에 쓰기 권한이 있어야 합니다. 직접 업데이트하는 경우 `settings.user.json`과 `cache/`를 함께 복사합니다.

## 실행되지 않을 때

터미널에서 `./overmax`를 실행해 처음 표시되는 오류를 확인한 뒤 아래 항목을 적용합니다.

| 증상 또는 확인 결과                                   | 해결 방법                                                                                                                                      |
| ----------------------------------------------------- | ---------------------------------------------------------------------------------------------------------------------------------------------- |
| `Permission denied`                                   | `chmod +x ./overmax`를 실행합니다.                                                                                                             |
| `Exec format error` 또는 `uname -m`이 `x86_64`가 아님 | 현재 배포 번들은 x86_64에서만 실행할 수 있습니다.                                                                                              |
| `GLIBC_2.39 not found` 또는 glibc 2.39 미만           | glibc 2.39 이상인 배포판에서 실행합니다. 시스템의 glibc 파일만 수동 교체하면 안 됩니다.                                                        |
| `ldd`에 `not found`가 표시됨                          | 표시된 공유 라이브러리를 제공하는 배포판 패키지를 설치합니다. `.so` 파일을 임의로 복사하지 마세요.                                             |
| `WAYLAND_DISPLAY is not set`                          | 로그아웃한 뒤 Wayland 세션으로 로그인하고 같은 세션의 터미널에서 실행합니다. X11 전용 세션은 현재 지원하지 않습니다.                           |
| `DISPLAY is not set` 또는 `X11 connect failed`        | XWayland를 활성화하고 게임과 Overmax를 같은 데스크톱 세션에서 실행합니다. 환경 변수 값을 임의로 만들지 마세요.                                 |
| 오류 없이 바로 종료됨                                 | 이미 실행 중인 Overmax가 있는지 `pgrep -a overmax`로 확인합니다. 없다면 `XDG_RUNTIME_DIR`이 설정된 정상적인 데스크톱 세션에서 다시 실행합니다. |
| `zwlr_layer_shell_v1 is unavailable`                  | `wlr-layer-shell`을 지원하는 compositor를 사용합니다. 패키지 하나를 추가하는 것만으로 compositor의 미지원 기능을 보완할 수는 없습니다.         |
| Vulkan adapter/device/surface 오류                    | GPU 제조사가 제공하는 Vulkan 드라이버와 로더를 설치하거나 갱신합니다. 진단 도구가 있다면 `vulkaninfo --summary`가 성공하는지도 확인합니다.     |
| `Composite` 또는 `MIT-SHM` 오류                       | XWayland의 XComposite와 MIT-SHM 확장이 활성화된 세션을 사용합니다. Gamescope 내부 세션은 현재 지원하지 않습니다.                               |
| 한글이 보이지 않거나 `fc-match` 실패                  | fontconfig와 한글 글꼴을 설치한 뒤 `fc-cache -f`를 실행합니다.                                                                                 |
| `DJMAX RESPECT V window not found`                    | 게임을 먼저 실행하고 Proton이 native Wayland가 아닌 XWayland를 사용하며 게임과 Overmax의 `DISPLAY`가 같은지 확인합니다.                        |
| 게임 창은 찾지만 오버레이가 정상 표시되지 않음        | 게임을 테두리 없는 전체화면으로 바꾸고 단일 출력에서 다시 확인합니다.                                                                          |
| 설정 또는 캐시 저장 시 권한 오류                      | bundle을 사용자 쓰기 권한이 있는 디렉터리에 다시 풉니다.                                                                                       |
| 업데이트 후 실행되지 않음                             | 새 bundle 전체를 다시 풀고 기존 `settings.user.json`과 `cache/`만 복사합니다. 이전 실행 파일이나 공유 라이브러리와 섞지 마세요.                |

x86_64가 아니거나 compositor가 `wlr-layer-shell`을 지원하지 않는 등 지원 범위 자체를 벗어난 환경은 현재 설정 변경만으로 실행할 수 없습니다.

## 현재 미지원 기능과 환경

- 창모드
- Gamescope 및 Steam Deck Gaming Mode
- Linux 시스템 트레이 아이콘

## 아직 검증 범위가 부족한 환경

다음 환경은 현재 호환성을 보장할 만큼 검증되지 않았습니다.

- compositor와 배포판별 차이
- GPU 제조사와 드라이버 조합
- 독점 전체화면
- fractional scaling 및 HiDPI 조합
- 여러 Proton 버전
