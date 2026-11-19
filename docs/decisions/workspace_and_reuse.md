# Workspace & Crate Reuse Decision Log

워크스페이스 크레이트를 Overmax 바깥(다른 도구, 다른 게임용 프로젝트, AI 에이전트가 만드는 신규 프로젝트)에서 재사용하는 방식에 관한 결정을 기록한다.

---

## 📋 Decision History

| 날짜 | 결정 | 이유 | 참조 |
|:---|:---|:---|:---|
| 2026-11-20 | crates.io 배포는 하지 않고, git 의존성 재사용만 지원 대상으로 둔다 | 외부 사용자가 아직 없고 독립 버저닝 비용(크레이트별 semver 관리, 릴리즈 동기화)을 감당할 이유가 없음. 재사용 요구는 `git = "..."` + `package = "overmax-capture"` 형태로 충분히 충족됨 | [module-contracts.md §6](../guides/module-contracts.md#6-재사용-가능한-크레이트) |
| 2026-11-20 | 재사용 후보를 `overmax_cv` / `overmax-frame` / `overmax-capture` 3개로 한정하고, 나머지 크레이트는 `publish = false`로 앱 전용임을 명시한다 | 에이전트가 레포를 훑을 때 "가져다 써도 되는 것"과 "앱 내부 구현"을 크레이트 문서와 Cargo 메타데이터만으로 판별할 수 있게 하기 위함. core/detect/engine/data/runtime/app은 DJMAX 도메인 타입·DB·UI에 묶여 있음 | 각 크레이트 `lib.rs` 크레이트 문서 |
| 2026-11-20 | 재사용 후보 안의 DJMAX 전용 코드(아틀라스 레이아웃, `set_enable_gpu_atlas`)는 지금은 옮기지 않고 크레이트 문서에 경계만 표기한다. 분리(이동 또는 feature 게이트)와 크레이트별 `examples/`는 후속 작업으로 미룬다 | 문서·메타데이터 변경은 동작 위험이 없고, 코드 경계 변경은 안정화된 캡처/디텍션 경로를 건드리므로 별도 근거와 크레이트 단위 커밋으로 진행해야 함 | [TASKS.md 13.4](../../TASKS.md) |
