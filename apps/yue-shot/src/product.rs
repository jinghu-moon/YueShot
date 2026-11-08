//! Product-owned installation identity. Delivery reads this from the built binary.
use crate::startup::Channel;

pub(super) fn info(channel: Channel) -> serde_json::Value {
    let name = channel.name();
    let display = match channel {
        Channel::Dev => "YueShot Dev",
        Channel::Beta => "YueShot Beta",
        Channel::Stable => "YueShot",
    };
    serde_json::json!({
        "product": "YueShot", "version": env!("CARGO_PKG_VERSION"),
        "channel": name, "architecture": "x64",
        "productId": "com.seeyuer.yueshot",
        "installationId": format!("com.seeyuer.yueshot.{name}.x64"),
        "aumid": aumid(channel), "displayName": display,
        "dataRelative": data_relative(channel),
        "installRelative": format!("Programs/Yue/YueShot/{name}"),
        "uninstallKey": format!("Software/Microsoft/Windows/CurrentVersion/Uninstall/com.seeyuer.yueshot.{name}.x64"),
        "registryKey": format!("Software/Yue/com.seeyuer.yueshot.{name}.x64"),
    })
}
pub(super) fn data_relative(channel: Channel) -> String {
    format!("Yue/YueShot/{}", channel.name())
}
pub(super) fn aumid(channel: Channel) -> String {
    format!("com.seeyuer.YueShot.{}", channel.name())
}
pub(super) fn instance_id(channel: Channel, canonical_root: &std::path::Path) -> String {
    let identity = canonical_root.to_string_lossy().to_lowercase();
    format!(
        "com.seeyuer.yueshot.{}.{}",
        channel.name(),
        &blake3::hash(identity.as_bytes()).to_hex()[..20]
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn channel_installations_have_distinct_stable_identifiers() {
        let products = [Channel::Dev, Channel::Beta, Channel::Stable].map(info);
        for field in [
            "installationId",
            "aumid",
            "displayName",
            "dataRelative",
            "installRelative",
            "uninstallKey",
            "registryKey",
        ] {
            let values: std::collections::HashSet<_> = products
                .iter()
                .map(|v| v[field].as_str().unwrap())
                .collect();
            assert_eq!(values.len(), 3, "{field}");
        }
        assert_eq!(products[1]["dataRelative"], "Yue/YueShot/beta");
        assert_eq!(products[2]["displayName"], "YueShot");
    }
}
