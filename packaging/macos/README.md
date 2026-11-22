# DMG background assets

Place the optional Finder background at `packaging/macos/background.png` with dimensions of exactly 660x440 pixels.

For a crisp Retina background, also provide `packaging/macos/background@2x.png` at exactly 1320x880 pixels. `workflow_macos.yml` combines both images into one multi-resolution TIFF. The 1x image is required when the optional 2x image is present.
