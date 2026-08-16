# HACS setup + auto-listing Docker container card

Do these in order tomorrow. Nothing here is urgent and nothing breaks your
existing dashboard — HACS just *adds* the ability to install custom cards.

## Part 1 — Install HACS (~10 min, one-time)

HACS can't be installed from inside the HA UI; you run a one-line script once,
then finish in the UI.

### Step 1: run the install script
1. HA → **Settings → Add-ons → Add-on Store**.
2. If you don't already have it, install the **"Advanced SSH & Web Terminal"**
   add-on (by Frenck). Open its **Configuration**, turn **Protection mode OFF**
   (needed so the terminal can reach the HA config folder), Start it, open **Terminal**.
   - (If you already have the "Terminal & SSH" add-on, use that.)
3. In the terminal, paste this and press Enter:

   ```
   wget -O - https://get.hacs.xyz | bash -
   ```

   You should see "INFO: Installation complete." at the end.

### Step 2: restart Home Assistant
- HA → **Settings → System → (top-right power icon) → Restart Home Assistant**.

### Step 3: add the HACS integration
1. HA → **Settings → Devices & Services → + Add Integration**.
2. Search **HACS**, click it.
3. Tick all the acknowledgement boxes → **Submit**.
4. It asks you to authorize via GitHub: it shows a code + opens github.com/login/device.
   Log in to GitHub, paste the code, authorize. (Any free GitHub account works.)
5. HACS now appears in the sidebar.

## Part 2 — Install the "auto-entities" card

1. Sidebar → **HACS**.
2. Search **auto-entities** (by Thomas Lovén) → open it → **Download** → confirm.
3. HA → **Settings → System → Restart Home Assistant** (or just reload the page;
   a restart is safest so the new card resource loads).

## Part 3 — Add the auto-listing container card

Edit your Servers dashboard → add a **Manual** card (Edit → + Add Card →
scroll to bottom → Manual) → paste this:

```yaml
type: custom:auto-entities
card:
  type: entities
  title: 🐳 All containers (by memory)
  state_color: true
filter:
  include:
    - entity_id: sensor.home_server_c_*_memory
    - entity_id: sensor.aipi_c_*_memory
sort:
  method: state
  numeric: true
  reverse: true
```

This automatically lists **every** Docker container across both boxes, sorted by
memory use, and updates itself as containers come and go — no manual editing ever.

Want CPU instead of memory? Change both `_memory` to `_cpu`.

## Notes
- The entity id pattern is `sensor.<node>_c_<container>_<metric>`, e.g.
  `sensor.aipi_c_jellyfin_memory`. Auto-entities matches them with the `*` wildcard.
- If the card says "Custom element doesn't exist: auto-entities", the resource
  didn't load yet — do a full HA restart and hard-refresh the browser.
