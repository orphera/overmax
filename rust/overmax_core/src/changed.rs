#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct Changed<T> {
    value: T,
}

impl<T> Changed<T> {
    pub fn new(initial: T) -> Self {
        Self { value: initial }
    }
}

impl<T: PartialEq> Changed<T> {
    /// 값을 업데이트하고, 만약 이전 값과 다르면 true를 반환합니다.
    pub fn update(&mut self, new_val: T) -> bool {
        if self.value != new_val {
            self.value = new_val;
            true
        } else {
            false
        }
    }

    /// 내부 값에 대한 참조를 반환합니다.
    pub fn get(&self) -> &T {
        &self.value
    }
}

impl<T> std::ops::Deref for Changed<T> {
    type Target = T;
    fn deref(&self) -> &Self::Target {
        &self.value
    }
}
