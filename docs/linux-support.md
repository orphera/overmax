# Linux 지원 안내

Overmax의 Linux 지원은 초기 단계입니다. Windows와 같은 범용 지원이 아니라, 아래 조건을 만족하는 Proton/XWayland 환경만 현재 지원 대상으로 봅니다.

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

배포 번들은 Ubuntu 24.04의 glibc 2.39 ABI를 기준으로 빌드합니다.

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

설정과 캐시는 실행 디렉터리에 저장됩니다. 기존 버전에서 옮길 때는 `settings.user.json`과 `cache/`를 함께 복사합니다.

## 현재 미지원 기능과 환경

- 창모드
- Gamescope 및 Steam Deck Gaming Mode
- 앱 자체 자동 업데이트
- Linux 시스템 트레이 아이콘

Linux에서는 앱 자체 자동 업데이트를 사용하지 않습니다. 곡 및 이미지 DB의 시작 시 업데이트는 공통 기능으로 유지됩니다. 새 앱 버전은 bundle을 직접 받아 교체해야 합니다.

## 아직 검증 범위가 부족한 환경

다음 환경은 미지원으로 확정한 것이 아니라, 현재 호환성을 보장할 만큼 검증되지 않았습니다.

- compositor와 배포판별 차이
- GPU 제조사와 드라이버 조합
- 독점 전체화면
- fractional scaling 및 HiDPI 조합
- 여러 Proton 버전
