# Stereo video

Window > Stereo Video opens the controls for a companion Video window.
Enable 3D follows the same audio playhead, clip offset and Video Sync Offset as the ordinary Video window.
The native player owns that playhead; the companion has no clock of its own.
The companion advances on logic-only ticks when the main editor is minimized or covered, without queuing UI texture uploads for every hidden frame.
The native shell opts out of waiting for its parent’s compositor frame callback only while a completed companion picture is playing; the existing logic requests pace the updates, and ordinary disabled playback keeps its original wait.
Each completed decode supplies the entire spatial pair with one presentation index and timestamp.
A seek, clip change or error never combines different decoder jobs into a pair.

The resolution readout is each eye's native film grid.
Media samples retain their resolution and sample aspect; only the desktop scales for its window and output.
Display Settings opens the desktop's display page.
Input packing and Swap left and right eyes are saved per movie path in the session's existing settings store.
Changing either never changes the movie file.
A source explicitly interpreted as 2D, or with no supported stereo mark, remains one flat picture.

Automatic input reads the container's MP4 st3d box for Motion JPEG and ProRes, including explicit mono.
H.264 automatic interpretation is visibly refused until the shared decoder carries codec stereo metadata: a container mark must never override an unexamined frame-packing SEI.
An explicit per-file input packing still presents H.264 through its existing decoder at the audio playhead.
A custom or unknown st3d arrangement is refused until its actual input packing is selected.
Codec SEI, temporal/interlaced pairing, multiview codecs and shape detection are not implemented by this adapter's automatic reader.
The controls state this coverage; choose the input packing for spatial pictures carrying other marks.
The shared sample copier owns side-by-side, top-and-bottom, rows, columns and checkerboard grids; the adapter performs no eye resampling.
Timecode burn-in remains an ordinary Video-window overlay.

The existing English and Brazilian Portuguese application catalogs remain authoritative.
The edition adds stereo-control translations in English, Polish, Brazilian Portuguese, French, German, Spanish, Japanese, Korean, Simplified Chinese and Traditional Chinese in the shared craft-i18n data checkout, in each language’s soundcraft.tsv.
The additional eight language choices say “stereo controls”; their ordinary application text falls back to English.
These are partial catalogs, not a claim that the entire application is translated.
