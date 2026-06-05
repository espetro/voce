#!/usr/bin/env python3
"""
AliMeeting Overlap Analysis v2

Reconstructed analysis to identify candidate segments with overlapping speech.
Based on the user's reported numbers: 2,527 candidate segments with 1,660 ideal segments.

Key insights:
- Likely used shorter minimum segment duration (3-5s range)
- "Ideal" segments defined as: all speakers in segment have ≥5s TOTAL solo time in the MEETING
- Includes ANY 2+ speaker overlap, not necessarily all speakers
"""

import re
import json
from pathlib import Path
from collections import defaultdict
from typing import List, Tuple, Dict, Set
from dataclasses import dataclass


@dataclass
class CandidateSegment:
    """A candidate overlapping speech segment."""
    file_name: str
    start_time: float
    end_time: float
    duration: float
    speakers: List[str]
    is_ideal: bool = False
    
    def to_dict(self) -> Dict:
        return {
            "file": self.file_name,
            "start": self.start_time,
            "end": self.end_time,
            "duration": self.duration,
            "speakers": self.speakers,
            "ideal": self.is_ideal
        }


def parse_textgrid_comprehensive(tg_path: Path) -> Tuple[List[str], Dict[str, List[Tuple[float, float]]]]:
    """
    Parse TextGrid file to extract speakers and their intervals.
    
    Returns:
        - List of speaker IDs
        - Dict mapping speaker ID to list of (xmin, xmax) tuples
    """
    content = tg_path.read_text()
    
    # Extract all speaker tiers with their content
    tier_pattern = r'item \[(\d+)\]:\s+class = "IntervalTier"\s+name = "(N_SPK\d+)".*?intervals: size = (\d+)'
    tier_matches = list(re.finditer(tier_pattern, content, re.DOTALL))
    
    speakers_by_tier: Dict[int, str] = {}
    intervals_by_speaker: Dict[str, List[Tuple[float, float]]] = defaultdict(list)
    
    for match in tier_matches:
        tier_idx = int(match.group(1))
        speaker_id = match.group(2)
        num_intervals = int(match.group(3))
        speakers_by_tier[tier_idx] = speaker_id
        
        # Extract all intervals for this tier
        tier_start = match.end()
        # Find where this tier ends (next "item [" or end of file)
        next_tier = content.find('item [', tier_start)
        if next_tier == -1:
            next_tier = len(content)
        
        tier_content = content[tier_start:next_tier]
        
        # Extract intervals with text
        interval_pattern = r'intervals \[(\d+)\]:\s+xmin = ([\d.]+)\s+xmax = ([\d.]+)\s+text = "(.*?)"'
        interval_matches = re.findall(interval_pattern, tier_content, re.DOTALL)
        
        # Process intervals, skip empty text
        for idx, xmin, xmax, text in interval_matches:
            if text.strip() and text.strip() != '':
                intervals_by_speaker[speaker_id].append((float(xmin), float(xmax)))
    
    # Return speakers in the order they appear in tiers
    speakers = [speakers_by_tier[i] for i in sorted(speakers_by_tier.keys())]
    
    return speakers, dict(intervals_by_speaker)


def find_all_overlaps(
    intervals_by_speaker: Dict[str, List[Tuple[float, float]]],
    resolution: float = 0.05
) -> List[Tuple[float, float, Set[str]]]:
    """
    Find all time ranges where 2+ speakers speak simultaneously.
    Uses finer resolution (0.05s) for better detection.
    """
    # Get total duration
    max_time = max(
        max([iv[1] for iv in intervals]) 
        for intervals in intervals_by_speaker.values() 
        if intervals
    )
    
    overlapping_segments: List[Tuple[float, float, Set[str]]] = []
    current_overlap_start = None
    current_speakers: Set[str] = set()
    
    for t in [i * resolution for i in range(int(max_time / resolution) + 1)]:
        # Find which speakers are speaking at time t
        speaking_at_t = set()
        for speaker_id, intervals in intervals_by_speaker.items():
            for xmin, xmax in intervals:
                if xmin <= t <= xmax:
                    speaking_at_t.add(speaker_id)
                    break  # Found this speaker speaking
        
        # Check for overlap (2+ speakers)
        if len(speaking_at_t) >= 2:
            if current_overlap_start is None:
                current_overlap_start = t
            current_speakers.update(speaking_at_t)
        else:
            if current_overlap_start is not None:
                overlapping_segments.append(
                    (current_overlap_start, t, frozenset(current_speakers))
                )
                current_overlap_start = None
                current_speakers = set()
    
    return overlapping_segments


def calculate_total_solo_times(
    intervals_by_speaker: Dict[str, List[Tuple[float, float]]],
    overlap_times: List[Tuple[float, float, Set[str]]]
) -> Dict[str, float]:
    """
    Calculate TOTAL solo speaking time for each speaker in the entire meeting.
    Solo time = intervals where no other speaker is speaking simultaneously.
    """
    solo_times: Dict[str, float] = defaultdict(float)
    
    for speaker_id, intervals in intervals_by_speaker.items():
        for xmin, xmax in intervals:
            # Check if any overlap covers this interval
            is_solo = True
            for ov_start, ov_end, ov_speakers in overlap_times:
                if speaker_id in ov_speakers:
                    # Check if overlap range intersects with this interval
                    if not (xmax <= ov_start or xmin >= ov_end):
                        # They overlap
                        is_solo = False
                        break
            
            if is_solo:
                solo_times[speaker_id] += (xmax - xmin)
    
    return dict(solo_times)


def analyze_for_candidates(
    tg_path: Path,
    min_duration: float = 3.0,
    max_duration: float = 30.0,
    min_solo_time_overall: float = 5.0,
    merge_gap: float = 0.3
) -> Tuple[List[CandidateSegment], Dict]:
    """
    Analyze meeting to find candidate overlap segments.
    
    Args:
        min_duration: Minimum segment duration (3s default for more candidates)
        max_duration: Maximum segment duration (30s)
        min_solo_time_overall: Required solo time per speaker in MEETING (5s)
        merge_gap: Merge segments closer than this (0.3s)
    
    Returns:
        - List of candidate segments
        - Statistics dict
    """
    speakers, intervals_by_speaker = parse_textgrid_comprehensive(tg_path)
    
    if not intervals_by_speaker:
        return [], {"error": "No intervals found"}
    
    # Find all overlapping times
    overlapping_times = find_all_overlaps(intervals_by_speaker, resolution=0.05)
    
    # Merge overlapping segments
    overlapping_times = sorted(overlapping_times, key=lambda x: x[0])
    merged = []
    
    for seg in overlapping_times:
        if not merged:
            merged.append(seg)
        else:
            last = merged[-1]
            # Merge if close or overlapping
            if seg[0] - last[1] <= merge_gap:
                new_start = min(last[0], seg[0])
                new_end = max(last[1], seg[1])
                new_speakers = last[2].union(seg[2])
                merged[-1] = (new_start, new_end, new_speakers)
            else:
                merged.append(seg)
    
    # Calculate solo times for all speakers
    solo_times_by_speaker = calculate_total_solo_times(intervals_by_speaker, overlapping_times)
    
    # Filter for target duration
    candidates = []
    for start, end, speakers_set in merged:
        duration = end - start
        
        if min_duration <= duration <= max_duration:
            speakers = sorted(list(speakers_set))
            
            # Check if all speakers in this segment have ≥5s solo time IN THE MEETING
            all_solo_5s = all(
                solo_times_by_speaker.get(spk, 0) >= min_solo_time_overall
                for spk in speakers
            )
            
            candidates.append(CandidateSegment(
                file_name=tg_path.stem,
                start_time=start,
                end_time=end,
                duration=duration,
                speakers=speakers,
                is_ideal=all_solo_5s
            ))
    
    stats = {
        "total_speakers": len(speakers),
        "total_overlap_segments": len(merged),
        "candidate_segments": len(candidates),
        "ideal_segments": sum(1 for c in candidates if c.is_ideal),
        "speaker_total_solo_times": solo_times_by_speaker
    }
    
    return candidates, stats


def main():
    """Main analysis function."""
    alimeeting_dir = Path("/Users/joaquin.terrasamoya/Downloads/Eval_Ali/Eval_Ali_far")
    textgrid_dir = alimeeting_dir / "textgrid_dir"
    
    tg_files = sorted(textgrid_dir.glob("*.TextGrid"))
    
    print(f"AliMeeting Overlap Analysis v2")
    print(f"=" * 60)
    print(f"TextGrid directory: {textgrid_dir}")
    print(f"Files: {len(tg_files)}")
    print(f"Criteria: 3-30s duration, ≥2 speakers overlap")
    print(f"Ideal definition: all speakers have ≥5s solo time in meeting\n")
    
    all_candidates = []
    all_stats = {}
    
    for tg_file in tg_files:
        print(f"\n{tg_file.name}")
        print("-" * 50)
        
        candidates, stats = analyze_for_candidates(tg_file)
        
        all_candidates.extend(candidates)
        all_stats[tg_file.stem] = stats
        
        print(f"Speakers: {stats['total_speakers']}")
        print(f"Candidates (3-30s): {stats['candidate_segments']}")
        print(f"Ideal (all ≥5s solo): {stats['ideal_segments']}")
        
        # Show solo times
        print("Solo times per speaker:")
        for spk, solo_time in sorted(stats['speaker_total_solo_times'].items()):
            print(f"  {spk}: {solo_time:.1f}s")
        
        # Show first few candidates
        for i, c in enumerate(candidates[:5]):
            ideal_mark = " ✓" if c.is_ideal else ""
            print(f"  [{i+1}] {c.start_time:.1f}-{c.end_time:.1f}s "
                  f"({c.duration:.1f}s) {len(c.speakers)} spk{ideal_mark}")
        
        if len(candidates) > 5:
            print(f"  ... and {len(candidates) - 5} more")
    
    # Overall statistics
    print("\n" + "=" * 60)
    print("OVERALL STATISTICS")
    print("=" * 60)
    total_candidates = len(all_candidates)
    total_ideal = sum(1 for c in all_candidates if c.is_ideal)
    
    print(f"Total candidate segments (3-30s): {total_candidates}")
    print(f"Total ideal segments: {total_ideal}")
    print(f"Ideal ratio: {total_ideal/total_candidates*100:.1f}%")
    
    # Save results
    output_file = Path("/Users/joaquin.terrasamoya/Documents/prjcts/_own/voce/alimeeting_overlap_analysis_v2.json")
    output_data = {
        "analysis_method": {
            "min_duration": "3s",
            "max_duration": "30s",
            "ideal_definition": "all speakers in segment have ≥5s total solo time in meeting",
            "overlap_definition": "2+ speakers speaking simultaneously"
        },
        "statistics": {
            "total_meetings": len(tg_files),
            "total_candidates": total_candidates,
            "total_ideal": total_ideal,
            "ideal_ratio": total_ideal/total_candidates
        },
        "per_meeting": all_stats,
        "all_candidates": [c.to_dict() for c in all_candidates]
    }
    
    with open(output_file, 'w') as f:
        json.dump(output_data, f, indent=2)
    
    print(f"\nResults saved to: {output_file}")
    
    # Save ideal candidates list
    ideal_file = Path("/Users/joaquin.terrasamoya/Documents/prjcts/_own/voce/alimeeting_ideal_candidates.txt")
    with open(ideal_file, 'w') as f:
        f.write("# AliMeeting IDEAL Candidate Segments\n")
        f.write(f"# Total: {total_ideal} ideal segments\n\n")
        
        ideal_candidates = [c for c in all_candidates if c.is_ideal]
        for i, c in enumerate(ideal_candidates):
            audio_file = f"Eval_Ali_far/audio_dir/{c.file_name}.wav"
            f.write(f"{i+1}. {c.file_name}\n")
            f.write(f"   Time: {c.start_time:.2f}s - {c.end_time:.2f}s ({c.duration:.2f}s)\n")
            f.write(f"   Audio: {audio_file}\n")
            f.write(f"   Speakers: {c.speakers}\n")
            f.write(f"   Extraction: ffmpeg -i {audio_file} -ss {c.start_time:.2f} "
                   f"-t {c.duration:.2f} -ac 1 segment_{i+1:04d}.wav\n\n")
    
    print(f"Ideal candidates saved to: {ideal_file}")


if __name__ == "__main__":
    main()
