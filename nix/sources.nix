{
  "aarch64-darwin" = {
    rustTarget = "aarch64-apple-darwin";
    skiaHash = "sha256-LIBVu1AsSsQ4UTtXTlhvRiQEvYCHDGFkWixVzCOK//g=";
    ffmpeg = {
      url = "https://ffmpeg.martin-riedl.de/download/macos/arm64/1785863997_9.0/ffmpeg.zip";
      hash = "sha256-UmfvFJ7g0ggFehsxaqwHm2YbBHZXTe5dp9IldpdzxgM=";
    };
    ffprobe = {
      url = "https://ffmpeg.martin-riedl.de/download/macos/arm64/1785863997_9.0/ffprobe.zip";
      hash = "sha256-d3j7tTP7YNMzbL2anlHs7XFljwILVwxyA1kMHEHUL1A=";
    };
  };
  "x86_64-darwin" = {
    rustTarget = "x86_64-apple-darwin";
    skiaHash = "sha256-cXJo4jvUVm7mz437ptPi1NMp3pPDBd2tbWNstW4VMkU=";
    ffmpeg = {
      url = "https://ffmpeg.martin-riedl.de/download/macos/amd64/1785871427_9.0/ffmpeg.zip";
      hash = "sha256-edFGY9iweNu8ON4Y1jow+KW/yGCvXf7n+M8+OHzxwCw=";
    };
    ffprobe = {
      url = "https://ffmpeg.martin-riedl.de/download/macos/amd64/1785871427_9.0/ffprobe.zip";
      hash = "sha256-ot0/Ln6zWhD6asQ7GowhiQ8nvuDcSobd7halfXLTiY0=";
    };
  };
  "aarch64-linux" = {
    rustTarget = "aarch64-unknown-linux-gnu";
    skiaHash = "sha256-vIGyqvTtcd2V3xrQgLeofBuqzmYw3H5Z1uwKKLdW6ic=";
    ffmpeg = {
      url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-09-30-13-08/ffmpeg-N-127021-ge0c94b2d1c-linuxarm64-gpl.tar.xz";
      hash = "sha256-sZRDCNYwaDn2ziIJWSDefm8x+eyQGDT+ElfgE9n8Oqc=";
    };
    ffprobe = null;
  };
  "x86_64-linux" = {
    rustTarget = "x86_64-unknown-linux-gnu";
    skiaHash = "sha256-75ziOO5OKTmxdD8DbxINB0BezbwDKYV9VUzDceyM0LI=";
    ffmpeg = {
      url = "https://github.com/BtbN/FFmpeg-Builds/releases/download/autobuild-2026-09-30-13-08/ffmpeg-N-127021-ge0c94b2d1c-linux64-gpl.tar.xz";
      hash = "sha256-OiqU5wR1IoeDNgRQGnlQXCMZrR2LL1V3xj8rG3pvK2Y=";
    };
    ffprobe = null;
  };
}
