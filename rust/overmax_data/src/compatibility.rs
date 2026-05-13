#[derive(Clone, Debug, Eq, PartialEq)]
pub struct DataCompatibility {
    pub settings_user_json: &'static str,
    pub record_db: &'static str,
    pub image_index_db: &'static str,
    pub songs_json: &'static str,
}

impl DataCompatibility {
    pub const fn current() -> Self {
        Self {
            settings_user_json: "settings.user.json",
            record_db: "cache/record.db",
            image_index_db: "cache/image_index.db",
            songs_json: "cache/songs.json",
        }
    }
}

#[cfg(test)]
mod tests {
    use super::DataCompatibility;

    #[test]
    fn preserves_python_file_contracts() {
        let compat = DataCompatibility::current();

        assert_eq!(compat.settings_user_json, "settings.user.json");
        assert_eq!(compat.record_db, "cache/record.db");
        assert_eq!(compat.image_index_db, "cache/image_index.db");
        assert_eq!(compat.songs_json, "cache/songs.json");
    }
}
