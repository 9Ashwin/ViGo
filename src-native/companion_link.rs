//! A narrow browser entry point: a public community pet ID, never a file or command.
pub const PREFIX: &str = "vigo://companion/import/";

pub fn website(url: &str) -> Result<String, String> {
    let id = url
        .strip_prefix(PREFIX)
        .filter(|id| crate::mascot::valid_id(id))
        .ok_or("导入链接无效，请从 Codex Pets 的伙伴详情页重试")?;
    Ok(format!("https://codex-pets.net/#/pets/{id}"))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn browser_links_resolve_to_the_official_community_only() {
        for id in [
            "aegis-sentinel",
            "pet-6af8bc5f4c0c81918e60784c437b7a4a",
            "Pet_42",
        ] {
            assert_eq!(
                website(&format!("{PREFIX}{id}")).unwrap(),
                format!("https://codex-pets.net/#/pets/{id}")
            );
        }
    }

    #[test]
    fn links_cannot_supply_paths_commands_or_another_download_origin() {
        for value in [
            "vigo://companion/import/",
            "vigo://other/import/pet-123",
            "codex://new?prompt=test",
            "https://codex-pets.net/#/pets/cat",
            "vigo://companion/import/../cat",
            "vigo://companion/import/%2e%2e",
            "vigo://companion/import/cat%2Fdog",
            "vigo://companion/import/cat?url=https://evil.test",
            "vigo://companion/import/cat#fragment",
            "vigo://companion/import/cat/extra",
            "vigo://companion/import/file:///tmp/cat.png",
            "vigo://companion/import/$(open Calculator)",
            "vigo://companion/import/猫",
        ] {
            assert!(website(value).is_err(), "{value}");
        }
        assert!(website(&format!("{PREFIX}{}", "a".repeat(97))).is_err());
    }
}
