# Game compatibility

SteamVR games tested with oneVR, and with stock WiVRn and xrizer for comparison. The list starts from Steam's top-selling VR games.

## Test setup

Fill in once per test round. A result is only comparable to the others of its round.

| | oneVR | Stock |
|---|---|---|
| Version | | WiVRn: , xrizer: |
| Setup | default install | as each project's README says |
| Distro | | |
| GPU and driver | | |
| Proton | | |
| Headset | | |
| Date | | |

## Results

- ✅ works out of the box
- ⚠️ works after a fix, described in Notes
- ❌ doesn't work
- 🛡️ blocked by anti-cheat, the same on any VR setup

Engine and VR API (OpenVR or OpenXR) are filled in where known. xrizer only handles OpenVR games, so a stock failure in an OpenXR game points at WiVRn rather than xrizer.

### Tier 1: test before launch

The games most buyers will try first.

| Game | App ID | Engine | VR API | oneVR | Stock | Notes |
|---|---|---|---|---|---|---|
| Half-Life: Alyx | 546560 | Source 2 | OpenVR | | | |
| Beat Saber | 620980 | Unity | | | | |
| Blade and Sorcery | 629730 | Unity | | | | |
| BONELAB | 1592190 | Unity | | | | |
| BONEWORKS | 823500 | Unity | OpenVR | | | |
| Gorilla Tag | 1533390 | Unity | | | | |
| VRChat | 438100 | Unity | | | | |
| The Elder Scrolls V: Skyrim VR | 611670 | Creation Engine | OpenVR | | | |
| Fallout 4 VR | 611660 | Creation Engine | OpenVR | | | |
| VTOL VR | 667970 | Unity | | | | |
| Walkabout Mini Golf VR | 1408230 | Unity | | | | |
| Hot Dogs, Horseshoes & Hand Grenades | 450540 | Unity | | | | |
| No Man's Sky | 275850 | | | | | |
| Phasmophobia | 739630 | Unity | | | | |
| PAYDAY: Aces High | 3456660 | | | | | |
| Into the Radius 2 | 2307350 | | | | | |
| Ghosts of Tabor | 1957780 | | | | | |
| Dungeons of Eternity | 3189340 | | | | | |
| Synth Riders | 885000 | Unity | | | | |
| SUPERHOT VR | 617830 | Unity | | | | |
| Pavlov | 555160 | Unreal Engine | | | | |
| Contractors | 963930 | Unreal Engine | | | | |
| The Walking Dead: Saints & Sinners | 916840 | Unreal Engine | | | | |
| Arizona Sunshine VR 2 | 1540210 | Unity | | | | |
| Job Simulator | 448280 | Unity | | | | |

### Tier 2: simulators

Flight and racing sims, a group that spends a lot on VR.

| Game | App ID | Engine | VR API | oneVR | Stock | Notes |
|---|---|---|---|---|---|---|
| Assetto Corsa | 244210 | | | | | |
| Assetto Corsa Competizione | 805550 | Unreal Engine | | | | |
| Le Mans Ultimate | 2399420 | | | | | |
| Automobilista 2 | 1066890 | | | | | |
| RaceRoom Racing Experience | 211500 | | | | | |
| DiRT Rally 2.0 | 690790 | | | | | |
| DCS World Steam Edition | 223750 | | | | | |
| Microsoft Flight Simulator (2020) | 1250410 | | OpenXR | | | |
| X-Plane 12 | 2014780 | | | | | |
| IL-2 Sturmovik: Battle of Stalingrad | 307960 | | | | | |
| Elite Dangerous | 359320 | | | | | |
| Project Wingman | 895870 | Unreal Engine | | | | |
| War Thunder | 236390 | | | | | |

### Tier 3: coverage

Other popular games, picked to cover more engines and genres.

| Game | App ID | Engine | VR API | oneVR | Stock | Notes |
|---|---|---|---|---|---|---|
| Tabletop Simulator | 286160 | Unity | | | | |
| Subnautica | 264710 | Unity | | | | |
| The Forest | 242760 | Unity | | | | |
| HITMAN World of Assassination | 1659040 | | | | | |
| Keep Talking and Nobody Explodes | 341800 | Unity | | | | |
| HARD BULLET | 1294760 | Unity | | | | |
| GORN | 578620 | Unity | | | | |
| The Thrill of the Fight - VR Boxing | 494150 | Unity | | | | |
| Metro Awakening | 2669410 | Unreal Engine | | | | |
| Moss VR | 846470 | Unreal Engine | | | | |
| Vertigo 2 | 843390 | Unity | | | | |
| Medal of Honor: Above and Beyond | 1402320 | Unreal Engine | | | | |
| Thief VR: Legacy of Shadow | 2800080 | Unreal Engine | | | | |
| Demeo x Dungeons & Dragons: Battlemarked | 3124340 | | | | | |

## How to test

1. Start the game from Steam with oneVR running and play for 5 minutes: menus, controllers in hand, one gameplay scene.
2. Mark ✅ only if nothing was changed for that game: no launch options, no config edits, no per-game fixes.
3. For Stock, use a separate setup with stock WiVRn and xrizer, set up as their READMEs say, and the same game version and Proton.
4. Write what failed in Notes, for example "black screen after the menu" or "controllers not tracked".
