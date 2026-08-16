# Resource-reporter — open items to revisit after home-server maintenance/reboot

Last updated: 2026-08-16, before kasper reboots home-server for maintenance.

## 1. GitHub push is PENDING (main open item)
- Local repo: ~/projects/resource-reporter — all committed on branch `main`,
  already scrubbed of the real tailnet hostname. Ready to push.
- Remote: KasperBosteels/ha-resource-reporter (SSH remote).
- BLOCKER: the dedicated key ~/.ssh/id_ed25519_github is configured in
  ~/.ssh/config for github.com, BUT GitHub does "server accepts key" then
  "Permission denied". That pattern = the public key is NOT registered on the
  GitHub *account* that owns the repo (or it's attached as a single-repo deploy
  key elsewhere).
- FIX: kasper adds this public key to github.com -> Settings -> SSH and GPG keys
  -> New SSH key -> type "Authentication Key", on the account that owns the repo:
      (see ~/.ssh/id_ed25519_github.pub on home-server)
  Then: cd ~/projects/resource-reporter && git push -u origin main
- WRINKLE: the remote already has a commit "init" (c1c51f2 shows locally in a way
  that suggests remote has content). If push is rejected as non-fast-forward,
  reconcile with:  git pull --rebase origin main   (or inspect first with
  git ls-remote origin) before pushing. Do NOT force-push without checking.
- Alternative if SSH stays broken: a GitHub Personal Access Token (PAT) over
  HTTPS works too — kasper can paste one and we push via
  https://github.com/KasperBosteels/ha-resource-reporter.git

## 2. Done this session (no action needed, just record)
- Both GitHub Actions runners daemonized: ~/github-actions-runners/
  - gh-runner-resource-reporter.service  (repo ha-resource-reporter)
  - gh-runner-muthur.service             (repo MuThUr)
  User systemd services, linger on, ExecStart = run.sh (svc.sh needs sudo/pw).
  Both were "Connected to GitHub, Listening for Jobs".
- Jellyfin (aipi): Abyss theme applied via branding CustomCss
  (@import jsdelivr AumGupta/abyss-jellyfin@v1.2.2), replaced the old Auroboros
  theme+logo. Themerr plugin installed+Active (LizardByte repo). Custom CSS only
  renders in web/mobile clients, NOT the Sony TV app.
- HACS "configuratiefout" was stale post-install state; fixed by an HA core
  restart (hacs_update entity now live).

## 3. After reboot, sanity checks
- systemctl --user status resource-reporter gh-runner-resource-reporter gh-runner-muthur
- confirm all resource-reporter devices back in Home Assistant
