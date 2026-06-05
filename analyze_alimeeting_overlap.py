#!/usr/bin/env python3
"""
AliMeeting Overlap Analysis

Analyzes AliMeeting dataset to identify:
1. Candidate segments (10-30s duration with overlapping speech)
2. Ideal segments (where all speakers have ≥5s solo time)

Methodology:
- Parse TextGrid files to extract speaker speaking intervals
- Detect time ranges where 2+ speakers speak simultaneously (overlap)
- Filter for segments in 10-30s duration range
- For each candidate, check if each speaker has ≥5s solo speaking time
- Generate report with statistics and candidate lists
"""

import re
import json
from pathlib import Path
from collections import defaultdict
from typing import List, Tuple, Dict, Set
from dataclasses import dataclass


@dataclass
class SpeakerInterval:
    """A speaking interval for a single speaker."""
    speaker_id: str
    xmin: float
    xmax: float
    text: str
    
    @property
    def duration(self) -> float:
        return self.xmax - self.xmin


@dataclass
class OverlapSegment:
    """A segment where multiple speakers speak simultaneously."""
    start_time: float
    end_time: float
    speakers: List[str]
    duration: float
    all_speakers_solo_5s: bool = False
    
    def to_dict(self) -> Dict:
        return {
            "start_time": self.start_time,
            "end_time": self.end_time,
            "duration": self.duration,
            "speakers": self.speakers,
            "all_speakers_solo_5s": self.all_speakers_solo_5s
        }


def parse_textgrid(tg_path: Path) -> Tuple[List[str], Dict[str, List[SpeakerInterval]]]:
    """
    Parse a Praat TextGrid file to extract speakers and their intervals.
    
    Returns:
        - List of speaker IDs (tier names like N_SPK8013)
        - Dict mapping speaker ID to list of intervals
    """
    content = tg_path.read_text()
    
    # Extract tier names (speakers)
    speaker_pattern = r'name = "(N_SPK\d+)"'
    speakers = re.findall(speaker_pattern, content)
    
    # Extract all intervals
    # Pattern matches: intervals [N]: \n xmin = X \n xmax = Y \n text = "..."
    interval_pattern = r'intervals \[(\d+)\]:\s+xmin = ([\d.]+)\s+xmax = ([\d.]+)\s+text = "(.*?)"'
    matches = re.findall(interval_pattern, content, re.DOTALL)
    
    # Group intervals by speaker
    # The TextGrid has all speaker tiers sequentially, so we need to track which speaker each interval belongs to
    intervals_by_speaker: Dict[str, List[SpeakerInterval]] = defaultdict(list)
    
    # Re-parse with speaker context
    # Split by speaker tier to maintain association
    tier_pattern = r'item \[(\d+)\]:\s+class = "IntervalTier"\s+name = "(N_SPK\d+)"'
    tier_matches = re.finditer(tier_pattern, content)
    
    for tier_match in tier_matches:
        speaker_id = tier_match.group(2)
        tier_start = tier_match.end()
        
        # Find next tier or end of file
        next_tier = content.find('item [', tier_start)
        if next_tier == -1:
            next_tier = len(content)
        
        tier_content = content[tier_start:next_tier]
        
        # Extract intervals for this speaker
        tier_interval_matches = re.findall(
            r'intervals \[(\d+)\]:\s+xmin = ([\d.]+)\s+xmax = ([\d.]+)\s+text = "(.*?)"',
            tier_content,
            re.DOTALL
        )
        
        for _, xmin, xmax, text in tier_interval_matches:
            if text.strip():  # Skip empty intervals (silence)
                intervals_by_speaker[speaker_id].append(
                    SpeakerInterval(
                        speaker_id=speaker_id,
                        xmin=float(xmin),
                        xmax=float(xmax),
                        text=text.strip()
                    )
                )
    
    return speakers, dict(intervals_by_speaker)


def find_overlapping_intervals(
    intervals_by_speaker: Dict[str, List[SpeakerInterval]],
    resolution: float = 0.1
) -> List[Tuple[float, float, Set[str]]]:
    """
    Find all time ranges where 2+ speakers are speaking simultaneously.
    
    Args:
        intervals_by_speaker: Dict mapping speaker ID to list of intervals
        resolution: Time resolution for sampling (default 0.1s)
    
    Returns:
        List of (start_time, end_time, set_of_speakers) tuples
    """
    # Get total duration
    max_time = max(
        max([iv.xmax for iv in intervals]) 
        for intervals in intervals_by_speaker.values() 
        if intervals
    )
    
    # Sample at resolution to detect overlaps
    overlapping_segments: List[Tuple[float, float, Set[str]]] = []
    current_overlap_start = None
    current_speakers: Set[str] = set()
    
    for t in [i * resolution for i in range(int(max_time / resolution) + 1)]:
        # Find which speakers are speaking at time t
        speaking_at_t = set()
        for speaker_id, intervals in intervals_by_speaker.items():
            for iv in intervals:
                if iv.xmin <= t <= iv.xmax:
                    speaking_at_t.add(speaker_id)
                    break  # Found this speaker
        
        # Check if we have overlap (2+ speakers)
        if len(speaking_at_t) >= 2:
            if current_overlap_start is None:
                current_overlap_start = t
            current_speakers.update(speaking_at_t)
        else:
            if current_overlap_start is not None:
                # Close current overlapping segment
                overlapping_segments.append(
                    (current_overlap_start, t, frozenset(current_speakers))
                )
                current_overlap_start = None
                current_speakers = set()
    
    return overlapping_segments


def merge_overlapping_segments(
    segments: List[Tuple[float, float, Set[str]]],
    min_gap: float = 0.5
) -> List[Tuple[float, float, Set[str]]]:
    """Merge overlapping or closely-spaced segments."""
    if not segments:
        return []
    
    # Sort by start time
    segments_sorted = sorted(segments, key=lambda x: x[0])
    
    merged = [segments_sorted[0]]
    
    for seg in segments_sorted[1:]:
        last = merged[-1]
        # If segments overlap or are within min_gap, merge them
        if seg[0] - last[1] <= min_gap:
            # Merge
            new_start = min(last[0], seg[0])
            new_end = max(last[1], seg[1])
            new_speakers = last[2].union(seg[2])
            merged[-1] = (new_start, new_end, new_speakers)
        else:
            merged.append(seg)
    
    return merged


def calculate_solo_times(
    intervals_by_speaker: Dict[str, List[SpeakerInterval]],
    overlap_segments: List[Tuple[float, float, Set[str]]],
    segment_start: float,
    segment_end: float
) -> Dict[str, float]:
    """
    Calculate solo speaking time for each speaker within a segment.
    
    Solo time = intervals where only this speaker is speaking.
    """
    solo_times = {spk: 0.0 for spk in intervals_by_speaker.keys()}
    
    # For each speaker, check their intervals
    for speaker_id, intervals in intervals_by_speaker.items():
        for iv in intervals:
            # Clip to segment bounds
            seg_start = max(iv.xmin, segment_start)
            seg_end = min(iv.xmax, segment_end)
            
            if seg_end <= seg_start:
                continue  # Interval outside segment
            
            # Check if any other speaker overlaps this interval
            is_solo = True
            for ov_start, ov_end, ov_speakers in overlap_segments:
                if (seg_start < ov_end and seg_end > ov_start and 
                    len(ov_speakers) >= 2 and speaker_id in ov_speakers):
                    # This interval overlaps with multi-speaker segment
                    is_solo = False
                    break
            
            if is_solo:
                solo_times[speaker_id] += (seg_end - seg_start)
    
    return solo_times


def analyze_meeting(
    tg_path: Path,
    min_duration: float = 10.0,
    max_duration: float = 30.0,
    min_solo_time: float = 5.0
) -> Tuple[List[OverlapSegment], Dict]:
    """
    Analyze a single meeting TextGrid file.
    
    Returns:
        - List of candidate overlap segments
        - Statistics dict
    """
    speakers, intervals_by_speaker = parse_textgrid(tg_path)
    
    if not intervals_by_speaker:
        return [], {"error": "No intervals found"}
    
    # Find all overlapping intervals in the entire meeting
    overlap_segments = find_overlapping_intervals(intervals_by_speaker, resolution=0.1)
    overlap_segments = merge_overlapping_segments(overlap_segments, min_gap=0.5)
    
    # Filter for target duration and check solo times
    candidates = []
    for start, end, speakers_set in overlap_segments:
        duration = end - start
        
        if min_duration <= duration <= max_duration:
            speakers = sorted(list(speakers_set))
            
            # Calculate solo times for each speaker in this segment
            solo_times = calculate_solo_times(
                intervals_by_speaker, overlap_segments, start, end
            )
            
            # Check if all speakers have ≥5s solo time
            all_solo_5s = all(
                solo_times.get(spk, 0) >= min_solo_time 
                for spk in speakers
            )
            
            candidates.append(OverlapSegment(
                start_time=start,
                end_time=end,
                duration=duration,
                speakers=speakers,
                all_speakers_solo_5s=all_solo_5s
            ))
    
    # Statistics
    stats = {
        "total_speakers": len(speakers),
        "total_overlap_segments": len(overlap_segments),
        "candidate_segments": len(candidates),
        "ideal_segments": sum(1 for c in candidates if c.all_speakers_solo_5s),
        "speaker_intervals_count": {
            spk: len(intervals) 
            for spk, intervals in intervals_by_speaker.items()
        }
    }
    
    return candidates, stats


def main():
    """Main analysis function."""
    alimeeting_dir = Path("/Users/joaquin.terrasamoya/Downloads/Eval_Ali/Eval_Ali_far")
    textgrid_dir = alimeeting_dir / "textgrid_dir"
    audio_dir = alimeeting_dir / "audio_dir"
    
    # Find all TextGrid files
    tg_files = sorted(textgrid_dir.glob("*.TextGrid"))
    
    print(f"AliMeeting Overlap Analysis")
    print(f"=" * 50)
    print(f"TextGrid directory: {textgrid_dir}")
    print(f"Files to analyze: {len(tg_files)}\n")
    
    all_candidates = []
    all_stats = {}
    
    for tg_file in tg_files:
        print(f"\nAnalyzing: {tg_file.name}")
        print("-" * 40)
        
        candidates, stats = analyze_meeting(tg_file)
        
        all_candidates.extend(candidates)
        all_stats[tg_file.stem] = stats
        
        print(f"  Total speakers: {stats['total_speakers']}")
        print(f"  Candidate segments (10-30s): {stats['candidate_segments']}")
        print(f"  Ideal segments (all ≥5s solo): {stats['ideal_segments']}")
        
        # Show first few candidates
        for i, c in enumerate(candidates[:3]):
            ideal_mark = " ✓ IDEAL" if c.all_speakers_solo_5s else ""
            print(f"    [{i+1}] {c.start_time:.1f}-{c.end_time:.1f}s "
                  f"({c.duration:.1f}s) {c.speakers}{ideal_mark}")
        
        if len(candidates) > 3:
            print(f"    ... and {len(candidates) - 3} more")
    
    # Overall statistics
    print("\n" + "=" * 50)
    print("OVERALL STATISTICS")
    print("=" * 50)
    total_candidates = len(all_candidates)
    total_ideal = sum(1 for c in all_candidates if c.all_speakers_solo_5s)
    
    print(f"Total candidate segments (10-30s): {total_candidates}")
    print(f"Total ideal segments (all ≥5s solo): {total_ideal}")
    print(f"Ideal ratio: {total_ideal/total_candidates*100:.1f}%")
    
    # Save detailed results
    output_file = Path("/Users/joaquin.terrasamoya/Documents/prjcts/_own/voce/alimeeting_overlap_analysis.json")
    output_data = {
        "statistics": {
            "total_meetings": len(tg_files),
            "total_candidate_segments": total_candidates,
            "total_ideal_segments": total_ideal,
            "ideal_ratio": total_ideal/total_candidates
        },
        "per_meeting_stats": all_stats,
        "all_candidates": [c.to_dict() for c in all_candidates]
    }
    
    with open(output_file, 'w') as f:
        json.dump(output_data, f, indent=2)
    
    print(f"\nDetailed results saved to: {output_file}")
    
    # Generate candidate file list
    candidates_file = Path("/Users/joaquin.terrasamoya/Documents/prjcts/_own/voce/alimeeting_candidates.txt")
    with open(candidates_file, 'w') as f:
        f.write(f"# AliMeeting Candidate Segments\n")
        f.write(f"# Total: {total_candidates} candidates, {total_ideal} ideal\n\n")
        
        for i, c in enumerate(all_candidates):
            ideal_mark = " [IDEAL]" if c.all_speakers_solo_5s else ""
            f.write(f"{i+1}. {c.start_time:.2f}-{c.end_time:.2f}s "
                   f"({c.duration:.2f}s) Speakers: {c.speakers}{ideal_mark}\n")
    
    print(f"Candidate list saved to: {candidates_file}")


if __name__ == "__main__":
    main()
