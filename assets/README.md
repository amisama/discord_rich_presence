# Asset keys

Upload your images at:
https://discord.com/developers/applications → your app → **Rich Presence → Art Assets**

The **name** you give an asset there is what you put into `large_image` / `small_image` in your rules. Recommended size: 512×512 PNG.

The default config references these keys:

- `vscode`
- `cursor`
- `warp`
- `terminal`
- `ssh`
- `chrome`
- `default` (idle)

If a key is empty or missing, that image just won't show up — the rest of the presence still works.
