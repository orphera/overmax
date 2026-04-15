# Agent Overview

이 에이전트는 DJMAX RESPECT V 오버레이 기반 추천 시스템의
정확도 개선, 성능 최적화, 안정성 향상을 목표로 한다.

---

# Primary Goals

- 인식 정확도 향상 (song / mode / difficulty / rate)
- 인게임 성능 영향 최소화
- 안정적인 상태 전이 (verified pipeline 유지)

---

# Context Usage Policy

- context.md를 현재 시스템 상태의 단일 source of truth로 사용한다
- context.md에 명시된 제약 조건을 절대 위반하지 않는다
- context.md에 없는 시스템은 존재한다고 가정하지 않는다

---

# Decision Policy

## 성능 vs 정확도

- 인게임 성능 영향이 있는 경우:
  → 정확도보다 성능을 우선한다

- 선곡 화면에서만 실행되는 로직:
  → 정확도 우선

---

## 인식 로직 수정

- 기존 파이프라인 (verified flow)을 깨지 않는 선에서 개선
- 단일 프레임 판단보다 history 기반 접근 우선
- OCR은 fallback 또는 검증 용도로만 사용

---

## 추천 시스템

- 현재 구조 (floor 기반)는 유지
- 새로운 기준 추가 시:
  → 기존 정렬 기준을 깨지 않도록 보완 방식으로 적용

---

# Constraints

- 메모리 접근 / 인젝션 금지
- 화면 캡처 기반 유지
- Python + 현재 라이브러리 스택 유지
- 실시간 처리 성능 저하 금지

---

# Failure Handling

- 확실하지 않은 경우:
  → 결과를 보류하거나 verified=False 유지

- 복수 해석 가능:
  → 조건별로 분리해서 제시

- 정보 부족:
  → 최소 질문만 생성 (1~2개)

---

# Output Format

기술 제안 시 반드시 다음 구조를 따른다:

1. 문제 정의
2. 원인 분석
3. 해결 방법 (옵션별)
4. 트레이드오프
5. 추천안

---

# Prohibited Actions

- 근거 없는 성능 개선 주장 금지
- 전체 리팩토링 제안 금지 (요청 시 제외)
- 기존 파이프라인 무시 금지
