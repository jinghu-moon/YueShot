# YueShot

## 0.1.3 (unreleased)

- Add on-demand update checks, authenticated downloads and normal-exit installation handoff.
- Embed the owner-provisioned beta Minisign public key; keep private keys outside the repository.
- Preserve custom installation directories containing spaces and verify the installed executable before restarting.
- Preserve existing screenshot behavior; production-signed online upgrade qualification remains pending.

## 0.1.2 (unreleased)

- Introduce channel-specific product installation identity, AUMID and shortcuts.
- Remove only login startup entries owned by the uninstalled executable/channel.
- Preserve existing screenshot behavior; no new screenshot features.
- Continue unsigned beta lifecycle validation; authenticated updates and public release remain gated.

## 0.1.1 (local candidate)

- Complete initial product separation and local NSIS delivery experiments.
- Add bounded metadata diagnostics and reject installer downgrades.
