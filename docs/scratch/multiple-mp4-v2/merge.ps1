# ============================================================
# Cycling Video Merger
#
# Set-ExecutionPolicy -Scope Process Bypass
#
# Original sections are stream-copied.
# Transition sections are encoded with NVIDIA NVENC, using the
# same codec (H.264 or HEVC) as the source clips so the final
# concat does not switch codecs mid-track.
#
# Transition:
#   wipeleft
#   3 seconds
#
# FFmpeg:
#   C:\Users\PC-Admin\AppData\Local\OVRLEY\vendor\ffmpeg\bin
# ============================================================

$ErrorActionPreference = "Stop"

# ------------------------------------------------------------
# Configuration
# ------------------------------------------------------------

$InputDir = $PSScriptRoot

$OutputFile = Join-Path `
    $InputDir `
    "final_cycling_video.mp4"

$TempDir = Join-Path `
    $InputDir `
    "merge_temp"

$TransitionDuration = 3.0
# $TransitionName = "wipeleft"
$TransitionName = "slideleft"

$FFmpegDir = "C:\Users\PC-Admin\AppData\Local\OVRLEY\vendor\ffmpeg\bin"

$FFmpeg = Join-Path `
    $FFmpegDir `
    "ffmpeg.exe"

$FFprobe = Join-Path `
    $FFmpegDir `
    "ffprobe.exe"

# The transition clips MUST be encoded with the same codec as the
# original (stream-copied) footage. The concat demuxer with "-c copy"
# does not transcode; if the transition parts used a different video
# codec than the originals, the resulting MP4 would switch codecs
# mid-track, which most decoders render as corruption/pixelation from
# that point on. The actual encoder/profile is chosen after probing
# the source clips, based on their codec (see Get-NvencEncoderForCodec).
$VideoEncoder = $null
$VideoProfile = $null

# Delete old temporary files before starting.
$CleanTempBeforeStart = $true

# Keep intermediate files for inspection.
$KeepTemporaryFiles = $true

# ------------------------------------------------------------
# Helper functions
# ------------------------------------------------------------

function Format-InvariantNumber {
    param(
        [double]$Value
    )

    return $Value.ToString(
        [System.Globalization.CultureInfo]::InvariantCulture
    )
}

function Write-Utf8NoBomFile {
    param(
        [string]$Path,
        [string[]]$Lines
    )

    # Important:
    # Set-Content -Encoding UTF8 in Windows PowerShell can write
    # a UTF-8 BOM. FFmpeg concat demuxer rejects that BOM.
    $Encoding = New-Object `
        System.Text.UTF8Encoding `
        ($false)

    [System.IO.File]::WriteAllLines(
        $Path,
        $Lines,
        $Encoding
    )
}

function Invoke-FFmpeg {
    param(
        [string[]]$Arguments
    )

    & $FFmpeg @Arguments

    if ($LASTEXITCODE -ne 0) {
        throw "FFmpeg failed with exit code $LASTEXITCODE"
    }
}

function Invoke-FFprobeJson {
    param(
        [string]$FilePath
    )

    $JsonText = & $FFprobe `
        "-v" "error" `
        "-print_format" "json" `
        "-show_format" `
        "-show_streams" `
        $FilePath 2>$null

    if ($LASTEXITCODE -ne 0) {
        throw "FFprobe failed for: $FilePath"
    }

    if ([string]::IsNullOrWhiteSpace($JsonText)) {
        throw "FFprobe returned no data for: $FilePath"
    }

    return (
        $JsonText -join "`n" |
        ConvertFrom-Json
    )
}

function Get-VideoInfo {
    param(
        [string]$FilePath
    )

    $Probe = Invoke-FFprobeJson `
        -FilePath $FilePath

    $VideoStream = @(
        $Probe.streams |
        Where-Object {
            $_.codec_type -eq "video"
        }
    )[0]

    $AudioStream = @(
        $Probe.streams |
        Where-Object {
            $_.codec_type -eq "audio"
        }
    )[0]

    if ($null -eq $VideoStream) {
        throw "No video stream found in: $FilePath"
    }

    $Duration = [double]::Parse(
        $Probe.format.duration,
        [System.Globalization.CultureInfo]::InvariantCulture
    )

    $FrameRate = $VideoStream.r_frame_rate

    if ([string]::IsNullOrWhiteSpace($FrameRate)) {
        $FrameRate = $VideoStream.avg_frame_rate
    }

    return [PSCustomObject]@{
        Path = $FilePath
        Name = Split-Path -Leaf $FilePath

        Duration = $Duration

        VideoCodec = $VideoStream.codec_name
        VideoProfile = $VideoStream.profile

        Width = [int]$VideoStream.width
        Height = [int]$VideoStream.height

        PixelFormat = $VideoStream.pix_fmt
        FrameRate = $FrameRate
        VideoTimeBase = $VideoStream.time_base

        HasAudio = ($null -ne $AudioStream)

        AudioCodec = if ($null -ne $AudioStream) {
            $AudioStream.codec_name
        } else {
            ""
        }

        AudioSampleRate = if ($null -ne $AudioStream) {
            $AudioStream.sample_rate
        } else {
            ""
        }

        AudioChannels = if ($null -ne $AudioStream) {
            [int]$AudioStream.channels
        } else {
            0
        }

        AudioChannelLayout = if ($null -ne $AudioStream) {
            $AudioStream.channel_layout
        } else {
            ""
        }
    }
}

function Get-NvencEncoderForCodec {
    param(
        [string]$CodecName,
        [string]$SourceProfile
    )

    switch ($CodecName) {
        "h264" {
            $Profile = "high"

            if (
                -not [string]::IsNullOrWhiteSpace($SourceProfile) -and
                $SourceProfile -match "Baseline|Main"
            ) {
                $Profile = $SourceProfile.ToLowerInvariant()
            }

            return [PSCustomObject]@{
                Encoder = "h264_nvenc"
                Profile = $Profile
            }
        }

        "hevc" {
            return [PSCustomObject]@{
                Encoder = "hevc_nvenc"
                Profile = "main"
            }
        }

        default {
            throw (
                "No NVENC encoder mapping for source codec " +
                "'$CodecName'. Add one before continuing, otherwise " +
                "the transition parts will not match the stream-copied " +
                "originals and playback will be corrupted."
            )
        }
    }
}

function Get-AnnexBBitstreamFilter {
    param(
        [string]$CodecName
    )

    switch ($CodecName) {
        "h264" { return "h264_mp4toannexb" }
        "hevc" { return "hevc_mp4toannexb" }

        default {
            throw (
                "No Annex B bitstream filter mapping for codec " +
                "'$CodecName'."
            )
        }
    }
}

function Convert-PartToTs {
    param(
        [string]$InputPath,
        [string]$BitstreamFilter,
        [string]$OutputPath
    )

    # MP4 stores SPS/PPS once per file (in avcC/hvcC), so concatenating
    # MP4 parts with "-c copy" keeps only the first part's parameter
    # set; any later part encoded with different low-level parameters
    # (e.g. the NVENC transitions vs. the camera's own encoder) then
    # gets decoded with the wrong parameter set, which looks like
    # pixelated corruption. MPEG-TS carries parameter sets inline per
    # segment, so remuxing through it avoids that problem.
    Invoke-FFmpeg -Arguments @(
        "-y",
        "-hide_banner",
        "-loglevel", "warning",

        "-i", $InputPath,

        "-map", "0:v:0",
        "-map", "0:a:0?",

        "-c", "copy",
        "-bsf:v", $BitstreamFilter,

        "-f", "mpegts",

        $OutputPath
    )
}

function Compare-ClipCompatibility {
    param(
        [object]$Reference,
        [object]$Candidate
    )

    $Problems = @()

    if ($Reference.Width -ne $Candidate.Width) {
        $Problems += (
            "Different width: {0} vs {1}" -f
            $Reference.Width,
            $Candidate.Width
        )
    }

    if ($Reference.Height -ne $Candidate.Height) {
        $Problems += (
            "Different height: {0} vs {1}" -f
            $Reference.Height,
            $Candidate.Height
        )
    }

    if ($Reference.PixelFormat -ne $Candidate.PixelFormat) {
        $Problems += (
            "Different pixel format: {0} vs {1}" -f
            $Reference.PixelFormat,
            $Candidate.PixelFormat
        )
    }

    if ($Reference.FrameRate -ne $Candidate.FrameRate) {
        $Problems += (
            "Different frame rate: {0} vs {1}" -f
            $Reference.FrameRate,
            $Candidate.FrameRate
        )
    }

    if ($Reference.HasAudio -ne $Candidate.HasAudio) {
        $Problems += "One clip has audio and the other does not"
    }

    if ($Reference.HasAudio) {
        if ($Reference.AudioSampleRate -ne $Candidate.AudioSampleRate) {
            $Problems += (
                "Different audio sample rate: {0} vs {1}" -f
                $Reference.AudioSampleRate,
                $Candidate.AudioSampleRate
            )
        }

        if ($Reference.AudioChannels -ne $Candidate.AudioChannels) {
            $Problems += (
                "Different audio channels: {0} vs {1}" -f
                $Reference.AudioChannels,
                $Candidate.AudioChannels
            )
        }

        if (
            $Reference.AudioChannelLayout -ne
            $Candidate.AudioChannelLayout
        ) {
            $Problems += (
                "Different audio channel layout: {0} vs {1}" -f
                $Reference.AudioChannelLayout,
                $Candidate.AudioChannelLayout
            )
        }
    }

    return $Problems
}

function New-OriginalSegment {
    param(
        [object]$Clip,
        [double]$StartSeconds,
        $DurationSeconds,
        [string]$OutputPath
    )

    $Arguments = @(
        "-y",
        "-hide_banner",
        "-loglevel", "warning",

        "-ss", (Format-InvariantNumber $StartSeconds),
        "-i", $Clip.Path
    )

    if ($null -ne $DurationSeconds) {
        $Arguments += @(
            "-t", (Format-InvariantNumber $DurationSeconds)
        )
    }

    $Arguments += @(
        "-map", "0:v:0",
        "-map", "0:a:0?",

        "-c", "copy",
        "-avoid_negative_ts", "make_zero",

        $OutputPath
    )

    Invoke-FFmpeg -Arguments $Arguments
}

function New-TransitionClip {
    param(
        [object]$CurrentClip,
        [object]$NextClip,
        [double]$DurationSeconds,
        [string]$TransitionName,
        [string]$OutputPath
    )

    # Every transition is built the same way regardless of its
    # position: the last $DurationSeconds of the current clip
    # crossfaded with the first $DurationSeconds of the next clip.
    $DurationText = Format-InvariantNumber $DurationSeconds

    $CurrentStartText = Format-InvariantNumber (
        $CurrentClip.Duration - $DurationSeconds
    )

    $Filter = (
        "[0:v]trim=start=0:end={0}," +
        "setpts=PTS-STARTPTS[v0];" +
        "[1:v]trim=start=0:end={0}," +
        "setpts=PTS-STARTPTS[v1];" +
        "[0:a]atrim=start=0:end={0}," +
        "asetpts=PTS-STARTPTS[a0];" +
        "[1:a]atrim=start=0:end={0}," +
        "asetpts=PTS-STARTPTS[a1];" +
        "[v0][v1]xfade=transition={1}:duration={0}:offset=0[vout];" +
        "[a0][a1]acrossfade=d={0}:c1=tri:c2=tri[aout]"
    ) -f @(
        $DurationText,
        $TransitionName
    )

    Invoke-FFmpeg -Arguments @(
        "-y",
        "-hide_banner",
        "-loglevel", "warning",

        "-ss", $CurrentStartText,
        "-i", $CurrentClip.Path,

        "-ss", "0",
        "-i", $NextClip.Path,

        "-filter_complex", $Filter,

        "-map", "[vout]",
        "-map", "[aout]",

        "-t", $DurationText,

        "-c:v", $VideoEncoder,
        "-preset", "p5",
        "-tune", "hq",
        "-rc", "vbr",
        "-cq", "19",
        "-b:v", "0",

        "-pix_fmt", "yuv420p",
        "-profile:v", $VideoProfile,

        "-c:a", "aac",
        "-ar", "48000",
        "-ac", "2",
        "-b:a", "192k",

        "-movflags", "+faststart",

        $OutputPath
    )
}

# ------------------------------------------------------------
# Validate FFmpeg
# ------------------------------------------------------------

Write-Host ""
Write-Host "Checking FFmpeg..." -ForegroundColor Cyan

if (-not (Test-Path -LiteralPath $FFmpeg)) {
    throw "FFmpeg was not found: $FFmpeg"
}

if (-not (Test-Path -LiteralPath $FFprobe)) {
    throw "FFprobe was not found: $FFprobe"
}

Write-Host "FFmpeg:  $FFmpeg"
Write-Host "FFprobe: $FFprobe"

# ------------------------------------------------------------
# Prepare temporary directory
# ------------------------------------------------------------

if (
    $CleanTempBeforeStart -and
    (Test-Path -LiteralPath $TempDir)
) {
    Write-Host ""
    Write-Host "Cleaning temporary directory..." `
        -ForegroundColor Yellow

    Remove-Item `
        -LiteralPath $TempDir `
        -Recurse `
        -Force
}

New-Item `
    -ItemType Directory `
    -Path $TempDir `
    -Force |
    Out-Null

# ------------------------------------------------------------
# Find input clips
# ------------------------------------------------------------

$OutputName = Split-Path `
    -Leaf `
    $OutputFile

$VideoFiles = @(
    Get-ChildItem `
        -LiteralPath $InputDir `
        -File `
        -Filter "*.mp4" |
    Where-Object {
        $_.Name -ne $OutputName
    } |
    Sort-Object Name
)

if ($VideoFiles.Count -lt 2) {
    throw "At least two MP4 files are required."
}

Write-Host ""
Write-Host "Found $($VideoFiles.Count) video files:" `
    -ForegroundColor Green

foreach ($File in $VideoFiles) {
    Write-Host "  $($File.Name)"
}

# ------------------------------------------------------------
# Read clip information
# ------------------------------------------------------------

$Clips = @()

foreach ($File in $VideoFiles) {
    Write-Host ""
    Write-Host "Inspecting: $($File.Name)" `
        -ForegroundColor Cyan

    $Info = Get-VideoInfo `
        -FilePath $File.FullName

    $Clips += $Info

    Write-Host (
        "Duration:       {0} seconds" -f
        (Format-InvariantNumber $Info.Duration)
    )

    Write-Host "Video codec:    $($Info.VideoCodec)"
    Write-Host "Resolution:     $($Info.Width)x$($Info.Height)"
    Write-Host "Pixel format:   $($Info.PixelFormat)"
    Write-Host "Frame rate:     $($Info.FrameRate)"
    Write-Host "Has audio:      $($Info.HasAudio)"

    if ($Info.HasAudio) {
        Write-Host "Audio codec:    $($Info.AudioCodec)"
        Write-Host "Audio rate:     $($Info.AudioSampleRate)"
        Write-Host "Audio channels: $($Info.AudioChannels)"
        Write-Host "Audio layout:   $($Info.AudioChannelLayout)"
    }
}

# ------------------------------------------------------------
# Pick a transition encoder that matches the source codec
# ------------------------------------------------------------
# The concat demuxer with "-c copy" never transcodes, so the
# encoded transition parts must use the exact same video codec as
# the stream-copied original parts. Otherwise the final MP4 flips
# codecs mid-track and playback turns pixelated after each cut.

$EncoderChoice = Get-NvencEncoderForCodec `
    -CodecName $Clips[0].VideoCodec `
    -SourceProfile $Clips[0].VideoProfile

$VideoEncoder = $EncoderChoice.Encoder
$VideoProfile = $EncoderChoice.Profile

$BitstreamFilter = Get-AnnexBBitstreamFilter `
    -CodecName $Clips[0].VideoCodec

Write-Host ""
Write-Host "Transition encoder: $VideoEncoder (profile: $VideoProfile)" `
    -ForegroundColor Cyan

$EncoderOutput = & $FFmpeg `
    "-hide_banner" `
    "-encoders" 2>$null

if (-not ($EncoderOutput -match [regex]::Escape($VideoEncoder))) {
    throw "Encoder '$VideoEncoder' was not found."
}

# ------------------------------------------------------------
# Check compatibility
# ------------------------------------------------------------

Write-Host ""
Write-Host "Checking clip compatibility..." `
    -ForegroundColor Cyan

$ReferenceClip = $Clips[0]
$CompatibilityProblems = @()

for ($Index = 1; $Index -lt $Clips.Count; $Index++) {
    $Problems = Compare-ClipCompatibility `
        -Reference $ReferenceClip `
        -Candidate $Clips[$Index]

    if ($Problems.Count -gt 0) {
        $CompatibilityProblems += (
            "Clip '{0}' compared with '{1}':" -f
            $Clips[$Index].Name,
            $ReferenceClip.Name
        )

        foreach ($Problem in $Problems) {
            $CompatibilityProblems += "  - $Problem"
        }
    }
}

if ($CompatibilityProblems.Count -gt 0) {
    $ReportPath = Join-Path `
        $TempDir `
        "compatibility_report.txt"

    Write-Utf8NoBomFile `
        -Path $ReportPath `
        -Lines $CompatibilityProblems

    Write-Host ""
    Write-Host "Compatibility problems detected:" `
        -ForegroundColor Red

    foreach ($Problem in $CompatibilityProblems) {
        Write-Host $Problem `
            -ForegroundColor Red
    }

    throw (
        "Clips are not compatible for stream-copy concatenation. " +
        "See: $ReportPath"
    )
}

Write-Host "All clips have compatible basic parameters." `
    -ForegroundColor Green

# ------------------------------------------------------------
# Validate transition duration
# ------------------------------------------------------------

$ShortestDuration = (
    $Clips |
    Measure-Object `
        -Property Duration `
        -Minimum
).Minimum

$MinimumRequiredDuration = `
    $TransitionDuration * 2

if ($ShortestDuration -le $MinimumRequiredDuration) {
    throw (
        "The shortest clip is $ShortestDuration seconds. " +
        "Each clip must be longer than " +
        "$MinimumRequiredDuration seconds."
    )
}

$TransitionText = Format-InvariantNumber `
    $TransitionDuration

# ------------------------------------------------------------
# Parts list
# ------------------------------------------------------------

$Parts = @()

# ------------------------------------------------------------
# Create original and transition parts
# ------------------------------------------------------------

for ($Index = 0; $Index -lt $Clips.Count; $Index++) {
    $CurrentClip = $Clips[$Index]

    $IsFirst = ($Index -eq 0)
    $IsLast = ($Index -eq ($Clips.Count - 1))

    Write-Host ""
    Write-Host "==================================================" `
        -ForegroundColor Cyan

    Write-Host (
        "Processing clip {0}/{1}: {2}" -f
        ($Index + 1),
        $Clips.Count,
        $CurrentClip.Name
    )

    Write-Host "==================================================" `
        -ForegroundColor Cyan

    # --------------------------------------------------------
    # Original segment
    # --------------------------------------------------------
    # Boundary clips only have a transition on one side, so the
    # start/end trimmed off to make room for it differs. The
    # transition itself (below) is always built the same way.

    $SegmentStart = if ($IsFirst) { 0 } else { $TransitionDuration }

    $SegmentDuration = if ($IsLast) {
        $null
    } else {
        $CurrentClip.Duration - $TransitionDuration - $SegmentStart
    }

    $OriginalPartPath = Join-Path `
        $TempDir `
        ("part-{0:D2}-original.mp4" -f ($Index + 1))

    $SegmentDurationText = if ($null -ne $SegmentDuration) {
        Format-InvariantNumber $SegmentDuration
    } else {
        "remaining clip"
    }

    Write-Host ""
    Write-Host "Creating original segment..."
    Write-Host "Start: $(Format-InvariantNumber $SegmentStart)"
    Write-Host "Duration: $SegmentDurationText"

    New-OriginalSegment `
        -Clip $CurrentClip `
        -StartSeconds $SegmentStart `
        -DurationSeconds $SegmentDuration `
        -OutputPath $OriginalPartPath

    $Parts += $OriginalPartPath

    # --------------------------------------------------------
    # Transition to next clip
    # --------------------------------------------------------

    if (-not $IsLast) {
        $NextClip = $Clips[$Index + 1]

        $TransitionPath = Join-Path `
            $TempDir `
            ("transition-{0:D2}-{1:D2}.mp4" -f (
                $Index + 1
            ), (
                $Index + 2
            ))

        Write-Host ""
        Write-Host "Creating transition to: $($NextClip.Name)"
        Write-Host "Effect: $TransitionName"
        Write-Host "Duration: $TransitionText seconds"

        New-TransitionClip `
            -CurrentClip $CurrentClip `
            -NextClip $NextClip `
            -DurationSeconds $TransitionDuration `
            -TransitionName $TransitionName `
            -OutputPath $TransitionPath

        $Parts += $TransitionPath
    }
}

# ------------------------------------------------------------
# Remux parts to MPEG-TS
# ------------------------------------------------------------
# Concatenating MP4 parts directly with "-c copy" only keeps the
# first part's SPS/PPS parameter set (MP4 stores it once, in
# avcC/hvcC), so later parts with different low-level encoder
# parameters get decoded with the wrong parameter set and look
# pixelated/corrupted. TS carries parameter sets inline per segment,
# so remux through it and use the concat protocol instead.

Write-Host ""
Write-Host "Remuxing parts to MPEG-TS..." `
    -ForegroundColor Cyan

$TsParts = @()

foreach ($Part in $Parts) {
    $TsPath = [System.IO.Path]::ChangeExtension($Part, ".ts")

    Convert-PartToTs `
        -InputPath $Part `
        -BitstreamFilter $BitstreamFilter `
        -OutputPath $TsPath

    $TsParts += $TsPath
}

Write-Host ""
Write-Host "Created parts:" `
    -ForegroundColor Green

foreach ($Part in $Parts) {
    Write-Host "  $(Split-Path -Leaf $Part)"
}

# ------------------------------------------------------------
# Concatenate parts
# ------------------------------------------------------------

Write-Host ""
Write-Host "Concatenating all parts..." `
    -ForegroundColor Cyan

$ConcatProtocolInput = "concat:" + ($TsParts -join "|")

Invoke-FFmpeg -Arguments @(
    "-y",
    "-hide_banner",
    "-loglevel", "warning",

    "-i", $ConcatProtocolInput,

    "-map", "0:v:0",
    "-map", "0:a:0?",

    "-c", "copy",
    "-movflags", "+faststart",

    $OutputFile
)

# ------------------------------------------------------------
# Validate final output
# ------------------------------------------------------------

Write-Host ""
Write-Host "Validating final output..." `
    -ForegroundColor Cyan

$FinalInfo = Get-VideoInfo `
    -FilePath $OutputFile

Write-Host ""
Write-Host "Final file:" `
    -ForegroundColor Green

Write-Host "  $OutputFile"
Write-Host ""

Write-Host (
    "Duration:     {0} seconds" -f
    (Format-InvariantNumber $FinalInfo.Duration)
)

Write-Host "Video codec:  $($FinalInfo.VideoCodec)"
Write-Host "Resolution:   $($FinalInfo.Width)x$($FinalInfo.Height)"
Write-Host "Pixel format: $($FinalInfo.PixelFormat)"
Write-Host "Frame rate:   $($FinalInfo.FrameRate)"
Write-Host "Audio codec:  $($FinalInfo.AudioCodec)"

# ------------------------------------------------------------
# Write report
# ------------------------------------------------------------

$ReportPath = Join-Path `
    $TempDir `
    "merge-report.txt"

$ReportLines = @(
    "Output: $OutputFile",
    "Transition: $TransitionName",
    "Transition duration: $TransitionDuration",
    "Final duration: $($FinalInfo.Duration)",
    "Video codec: $($FinalInfo.VideoCodec)",
    "Resolution: $($FinalInfo.Width)x$($FinalInfo.Height)",
    "Pixel format: $($FinalInfo.PixelFormat)",
    "Frame rate: $($FinalInfo.FrameRate)",
    "Audio codec: $($FinalInfo.AudioCodec)",
    "",
    "Parts:"
)

foreach ($Part in $Parts) {
    $ReportLines += "  $Part"
}

Write-Utf8NoBomFile `
    -Path $ReportPath `
    -Lines $ReportLines

# ------------------------------------------------------------
# Cleanup
# ------------------------------------------------------------

if (-not $KeepTemporaryFiles) {
    Write-Host ""
    Write-Host "Removing temporary files..." `
        -ForegroundColor Yellow

    Remove-Item `
        -LiteralPath $TempDir `
        -Recurse `
        -Force
}

Write-Host ""
Write-Host "==================================================" `
    -ForegroundColor Green

Write-Host "Merge completed successfully." `
    -ForegroundColor Green

Write-Host "Output:"
Write-Host $OutputFile

Write-Host "==================================================" `
    -ForegroundColor Green
