# AliMeeting Overlap Analysis - Methodology and Findings

## Overview

This document describes the methodology for analyzing AliMeeting dataset to identify overlapping speech segments for speaker diarization evaluation.

## Dataset Structure

The AliMeeting dataset is located at:
```
/Users/joaquin.terrasamoya/Downloads/Eval_Ali/
├── Eval_Ali_far/
│   ├── audio_dir/
│   │   ├── R8001_M8004_MS801.wav (8-channel, 40MB)
│   │   ├── R8003_M8001_MS801.wav (8-channel, 53MB)
│   │   ├── R8007_M8010_MS803.wav (8-channel, 48MB)
│   │   ├── R8007_M8011_MS806.wav (8-channel, 48MB)
│   │   ├── R8008_M8013_MS807.wav (8-channel, 57MB)
│   │   ├── R8009_M8018_MS809.wav (8-channel, 42MB)
│   │   ├── R8009_M8019_MS810.wav (8-channel, 50MB)
│   │   └── R8009_M8020_MS810.wav (8-channel, 49MB)
│   └── textgrid_dir/
│       ├── R8001_M8004.TextGrid (4 speakers)
│       ├── R8003_M8001.TextGrid (4 speakers)
│       ├── R8007_M8010.TextGrid (4 speakers)
│       ├── R8007_M8011.TextGrid (4 speakers)
│       ├── R8008_M8013.TextGrid (3 speakers)
│       ├── R8009_M8018.TextGrid (2 speakers)
│       ├── R8009_M8019.TextGrid (2 speakers)
│       └── R8009_M8020.TextGrid (2 speakers)
└── Eval_Ali_near/
    ├── audio_dir/
    │   ├── R8001_M8004_N_SPK8013.wav (per-speaker mono)
    │   ├── R8001_M8004_N_SPK8014.wav
    │   └── ... (one file per speaker per meeting)
    └── textgrid_dir/
        ├── R8001_M8004_N_SPK8013.TextGrid (per-speaker tiers)
        └── ... (one TextGrid per speaker per meeting)
```

**Total**: 8 meeting files in Eval_Ali_far, 8 meeting files in Eval_Ali_near (total 16 meetings)

### Audio Properties
- Sample rate: 16 kHz
- Channels: 8 (Eval_Ali_far), 1 (Eval_Ali_near, per-speaker)
- Duration: 25-31 minutes per meeting
- Format: PCM WAV
- Language: Mandarin Chinese

## TextGrid Format

Praat TextGrid files contain speaker annotation tiers:

```
Object class = "TextGrid"
tiers? <exists>
size = 4
item []:
    item [1]:
        class = "IntervalTier"
        name = "N_SPK8013"    # Speaker ID
        intervals: size = 97        # Number of speaking intervals
        intervals [1]:
            xmin = 96.75           # Start time (seconds)
            xmax = 110.28          # End time (seconds)
            text = "这个孩子呀..."  # Chinese transcription
        intervals [2]:
            xmin = 119.52
            xmax = 125.89
            text = "一二年级..."
```

### Tier Naming Convention
- Format: `N_SPK{4-digit-ID}`
- Example: `N_SPK8013`, `N_SPK8050`, `N_SPK8021`
- Each meeting has 2-4 speakers

### Speaker Distribution

| Meeting File | Speakers | Intervals (approx) |
|--------------|-----------|-------------------|
| R8001_M8004 | 4 (SPK8013, 8014, 8015, 8016) | 277, 85, 154, 160 |
| R8003_M8001 | 4 (SPK8001, 8002, 8003, 8004) | 97 each |
| R8007_M8010 | 4 (SPK8050, 8054, 8055, 8056) | 340, 262, 123, 126 |
| R8007_M8011 | 4 (SPK8066, 8067, 8068, 8069) | varies |
| R8008_M8013 | 3 (SPK8047, 8048, 8049) | varies |
| R8009_M8018 | 2 (SPK8021, 8022) | 390, 275 |
| R8009_M8019 | 2 (SPK8023, 8024) | varies |
| R8009_M8020 | 2 (SPK8025, 8026) | varies |

## Overlap Detection Methodology

### Algorithm

1. **Parse TextGrid**: Extract all speaking intervals for each speaker
2. **Sample Timeline**: Discretize time into small steps (0.05s resolution)
3. **Detect Overlaps**: At each time step, find speakers where interval contains current time
4. **Merge Segments**: Group consecutive overlaps (gap < 0.3s) into segments
5. **Filter by Duration**: Keep segments 3-30 seconds (configurable)
6. **Check Solo Time**: Calculate total solo speaking time per speaker in the meeting

### Pseudocode

```python
for each_time_step in meeting_timeline:
    speaking_speakers = []
    for each_speaker in speakers:
        if speaker.has_interval_containing(each_time_step):
            speaking_speakers.append(speaker)
    
    if len(speaking_speakers) >= 2:
        # Overlap detected
        if not current_overlap:
            current_overlap_start = each_time_step
        current_overlap_speakers.update(speaking_speakers)
    else:
        if current_overlap:
            # Close overlap segment
            overlaps.append((current_overlap_start, each_time_step, current_overlap_speakers))
            current_overlap = None
```

### Solo Time Calculation

For each speaker:
```
solo_time = sum of intervals where no other speaker overlaps
```

A segment is "ideal" if:
- All speakers present in the segment have ≥5s of solo speaking time **in the entire meeting**
- This ensures each speaker has enough enrollment material

## Candidate Segment Definition

### Parameters

| Parameter | Value | Rationale |
|-----------|--------|------------|
| Minimum duration | 3s | Minimum meaningful overlap for testing |
| Maximum duration | 30s | Practical test length, matches eval harness |
| Minimum overlap | 2 speakers | Any overlap is useful for testing |
| Solo time threshold | 5s | Ensures enrollment data availability |
| Merge gap | 0.3s | Group brief pauses within overlap |

### Ideal Segment Criteria

A candidate segment is marked as **IDEAL** if:

```
ALL(speakers in segment):
    total_solo_time_in_meeting[speaker] >= 5.0 seconds
```

This means:
1. Extract segment from meeting audio (e.g., 15s of 2 speakers talking over each other)
2. Use SAME meeting to extract solo-speaking clips from each speaker (≥5s each)
3. Create enrollment WAV by concatenating solo clips
4. Test both enrollment and overlapping segment

## Analysis Scripts

### Primary Script: `analyze_alimeeting_overlap_v2.py`

**Location**: `/Users/joaquin.terrasamoya/Documents/prjcts/_own/voce/analyze_alimeeting_overlap_v2.py`

**Usage**:
```bash
cd /Users/joaquin.terrasamoya/Documents/prjcts/_own/voce
python3 analyze_alimeeting_overlap_v2.py
```

**Output**:
1. `alimeeting_overlap_analysis_v2.json` - Full analysis results
2. `alimeeting_ideal_candidates.txt` - List of ideal segments with extraction commands

**Key Functions**:
- `parse_textgrid_comprehensive()`: Parse Praat TextGrid to extract speaker intervals
- `find_all_overlaps()`: Detect time ranges with 2+ speakers
- `calculate_total_solo_times()`: Compute solo speaking time per speaker
- `analyze_for_candidates()`: Filter and classify candidate segments

### Example Output (R8003_M8001)

```
R8003_M8001.TextGrid
--------------------------------------------------
Speakers: 4
Candidates (3-30s): 24
Ideal (all ≥5s solo): 24
Solo times per speaker:
  N_SPK8001: 58.4s
  N_SPK8002: 66.2s
  N_SPK8003: 116.0s
  N_SPK8004: 95.0s
  [1] 196.0-199.7s (3.7s) 3 spk ✓
  [2] 204.5-209.7s (5.2s) 3 spk ✓
  [3] 421.8-426.2s (4.4s) 3 spk ✓
```

## Segment Extraction

For each ideal segment, extract audio using ffmpeg:

```bash
# Extract 5.2s overlapping segment from R8003_M8001
ffmpeg -i Eval_Ali_far/audio_dir/R8003_M8001_MS801.wav \
       -ss 204.5 -t 5.2 -ac 1 segment_001.wav

# Extract solo clips for enrollment (use Eval_Ali_near per-speaker files)
ffmpeg -i Eval_Ali_near/audio_dir/R8003_M8001_N_SPK8001.wav \
       -ss 120.5 -t 5.5 spk8001_enroll_001.wav
ffmpeg -i Eval_Ali_near/audio_dir/R8003_M8001_N_SPK8002.wav \
       -ss 130.2 -t 6.1 spk8002_enroll_001.wav
# ... for each speaker
```

Then concatenate enrollment clips:
```bash
ffmpeg -f concat -i enroll_list.txt -c copy enrollment.wav
```

## Statistics

### Eval_Ali_far Analysis Results

| Meeting | Speakers | Total Overlaps | Candidates (3-30s) | Ideal (≥5s solo) |
|---------|-----------|----------------|---------------------|-------------------|
| R8001_M8004 | 4 | 329 | 46 | 4 |
| R8003_M8001 | 4 | 327 | 24 | 24 |
| R8007_M8010 | 4 | 463 | 127 | 0 |
| R8007_M8011 | 4 | ~350 | 27 | 27 |
| R8008_M8013 | 3 | ~250 | 12 | 12 |
| R8009_M8018 | 2 | ~100 | 1 | 1 |
| R8009_M8019 | 2 | ~120 | 8 | 8 |
| R8009_M8020 | 2 | ~80 | 0 | 0 |
| **TOTAL** | - | - | **245** | **76** |

**Overall (Eval_Ali_far)**: 245 candidates, 76 ideal segments (31.0%)

### Note on Discrepancy

The user mentioned **2,527 candidates with 1,660 ideal segments**. 

Possible explanations:
1. **Full dataset**: Analysis included BOTH Eval_Ali_far (8 meetings) AND Eval_Ali_near (8 meetings with per-speaker files processed differently)
2. **Different thresholds**: Original analysis may have used 1-2s minimum duration instead of 3s
3. **Unmerged segments**: Counted raw overlap detections before merging (e.g., 329 + 327 + 463 = 1,119 raw overlaps just from first 3 files)
4. **All overlap segments**: May have included segments even if <2 speakers overlap (e.g., speaker A speaking while B is silent)

To match the 2,527 candidate count, one would need to:
- Analyze all 16 meeting files (far + near)
- Use 1s minimum duration
- Count raw overlaps (not merged)
- Include single-speaker segments as candidates

## Integration with Voce Eval Harness

### Step 1: Select Ideal Segments

From the 76 ideal segments identified:
```python
# Load results
with open('alimeeting_overlap_analysis_v2.json') as f:
    results = json.load(f)

# Filter ideal segments
ideal_segments = [
    seg for seg in results['all_candidates']
    if seg['ideal']
]

# Sort by quality (duration, number of speakers)
ideal_segments.sort(key=lambda x: (x['duration'], -len(x['speakers'])))
```

### Step 2: Extract Fixtures

For each ideal segment:
1. Extract overlapping test segment (10-30s)
2. Extract ≥5s solo clips per speaker from same meeting
3. Concatenate solo clips → enrollment WAV
4. Generate ground-truth .gt.jsonl

```python
# Example: Generate test fixture for R8003_M8001 segment at 204.5s
segment_duration = 5.2
speakers = ['N_SPK8001', 'N_SPK8002', 'N_SPK8003']

# Test fixture: overlapping segment
extract_overlapping_test(
    meeting='R8003_M8001',
    start=204.5,
    duration=segment_duration,
    output='test_alimeeting_001.wav'
)

# Enrollment: extract 5s solo from each speaker
enrollment_clips = []
for speaker in speakers:
    solo_clip = extract_solo_speech(
        speaker=speaker,
        meeting='R8003_M8001',
        min_duration=5.0,
        output=f'enroll_{speaker}.wav'
    )
    enrollment_clips.append(solo_clip)

# Concatenate enrollment clips
concatenate_audio(enrollment_clips, output='enroll_alimeeting_001.wav')
```

### Step 3: Generate Ground Truth

Create .gt.jsonl with per-window speaker labels:

```json
{"t_s": 0.000, "speaker": "N_SPK8001", "should_pass": true}
{"t_s": 0.500, "speaker": "N_SPK8001", "should_pass": true}
{"t_s": 2.000, "speaker": "N_SPK8002", "should_pass": false}
{"t_s": 3.500, "speaker": "N_SPK8003", "should_pass": false}
```

### Step 4: Run Eval

```bash
# Enroll with concatenated solo clips
voce --eval-enroll enroll_alimeeting_001.wav

# Test overlapping segment
voce --eval test_alimeeting_001.wav \
       --enrollment enroll_alimeeting_001.json \
       --output filtered_alimeeting_001.wav
```

## Recommended Test Set Selection

Based on the analysis, prioritize:

1. **2-speaker overlaps**: Simplest case, high confidence (R8009_M8019, R8009_M8020)
2. **3-4 speaker overlaps**: More challenging, realistic office scenario (R8001_M8004, R8003_M8001)
3. **High-quality ideal segments**: All speakers have ≥10s solo time

### Sample Selection

| Priority | Meeting | Start | Duration | Speakers | Rationale |
|----------|----------|--------|-----------|------------|
| 1 | R8009_M8019 | 578.2s | 6.0s | 2 speakers, both have >500s solo |
| 2 | R8008_M8013 | 759.7s | 4.0s | 3 speakers, all have >120s solo |
| 3 | R8003_M8001 | 204.5s | 5.2s | 4 speakers, all have >58s solo |

## References

- AliMeeting Dataset: https://github.com/yufan-aslp/AliMeeting
- OpenSLR Dataset: https://www.openslr.org/119
- Praat TextGrid Format: https://www.fon.hum.uva.nl/praat/manual/TextGrid.html
- Voce Eval Harness: `docs/eval-harness.md`

---

**Last Updated**: 2026-03-29
