"""
TextGrid Parsing Utility for AliMeeting Dataset

This module provides functions to parse Praat TextGrid files, extract speaker
intervals, find overlaps, and calculate solo speaking time.

Compatible with the textgrid library (not praatio).

Example:
    >>> import parse_textgrid as tg
    >>> textgrid = tg.parse_textgrid("R8001_M8004.TextGrid")
    >>> intervals = tg.extract_speaker_intervals(textgrid[0])
    >>> overlaps = tg.find_overlaps({"SPK001": intervals}, 0.0, 60.0)
"""

from dataclasses import dataclass
from typing import Dict, List, Optional

# textgrid library imports
import textgrid as tg_lib


@dataclass
class Interval:
    """Represents a time interval in a TextGrid tier.

    Attributes:
        start: Start time in seconds
        end: End time in seconds
        text: Label text (empty string for silence)

    Example:
        >>> interval = Interval(0.0, 1.5, "hello")
        >>> interval.duration
        1.5
        >>> 0.75 in interval
        True
    """

    start: float
    end: float
    text: str

    @property
    def duration(self) -> float:
        """Return interval duration in seconds."""
        return self.end - self.start

    def __contains__(self, time: float) -> bool:
        """Check if a time falls within this interval."""
        return self.start <= time < self.end

    def overlaps_with(self, other: "Interval") -> bool:
        """Check if this interval overlaps with another.

        Example:
            >>> i1 = Interval(0.0, 1.0, "a")
            >>> i2 = Interval(0.5, 1.5, "b")
            >>> i1.overlaps_with(i2)
            True
        """
        return self.start < other.end and other.start < self.end

    def overlap(self, other: "Interval") -> Optional["Interval"]:
        """Return the overlapping region with another interval, or None."""
        overlap_start = max(self.start, other.start)
        overlap_end = min(self.end, other.end)
        if overlap_start < overlap_end:
            return Interval(overlap_start, overlap_end, self.text)
        return None

    @classmethod
    def merge(cls, intervals: List["Interval"]) -> List["Interval"]:
        """Merge overlapping or adjacent intervals.

        Example:
            >>> i1 = Interval(0.0, 1.0, "a")
            >>> i2 = Interval(0.9, 2.0, "a")
            >>> merged = Interval.merge([i1, i2])
            >>> len(merged)
            1
            >>> merged[0].end
            2.0
        """
        if not intervals:
            return []

        # Sort by start time
        sorted_intervals = sorted(intervals, key=lambda x: x.start)
        merged_intervals = [sorted_intervals[0]]

        for current in sorted_intervals[1:]:
            last = merged_intervals[-1]
            if current.start <= last.end:
                # Merge intervals
                last.end = max(last.end, current.end)
            else:
                merged_intervals.append(current)

        return merged_intervals


@dataclass
class Overlap:
    """Represents a time region where multiple speakers are speaking simultaneously.

    Attributes:
        start: Start time in seconds
        end: End time in seconds
        speakers: List of speaker IDs involved in the overlap

    Example:
        >>> overlap = Overlap(10.0, 12.5, ["SPK001", "SPK002"])
        >>> overlap.duration
        2.5
        >>> overlap.speakers
        ['SPK001', 'SPK002']
    """

    start: float
    end: float
    speakers: List[str]

    @property
    def duration(self) -> float:
        """Return overlap duration in seconds."""
        return self.end - self.start

    def __contains__(self, time: float) -> bool:
        """Check if a time falls within this overlap region."""
        return self.start <= time < self.end


def parse_textgrid(filepath: str) -> tg_lib.TextGrid:
    """Read a Praat TextGrid file and return the textgrid library object.

    Args:
        filepath: Path to the TextGrid file

    Returns:
        textgrid.TextGrid object containing all tiers

    Raises:
        FileNotFoundError: If the file does not exist
        Exception: If the file cannot be parsed

    Example:
        >>> textgrid = parse_textgrid("R8001_M8004.TextGrid")
        >>> len(textgrid)
        4  # Number of tiers
        >>> textgrid[0].name
        'N_SPK8013'
    """
    import os

    if not os.path.exists(filepath):
        raise FileNotFoundError(f"TextGrid file not found: {filepath}")

    try:
        textgrid = tg_lib.TextGrid.fromFile(filepath)
        return textgrid
    except Exception as e:
        raise Exception(f"Failed to parse TextGrid file {filepath}: {e}")


def extract_speaker_intervals(tier: tg_lib.IntervalTier) -> List[Interval]:
    """Extract non-empty (speech) intervals from a speaker tier.

    Filters out intervals with empty text (silence) or whitespace-only text.

    Args:
        tier: A textgrid IntervalTier object representing a speaker

    Returns:
        List of Interval objects with non-empty text

    Example:
        >>> tier = textgrid[0]  # First speaker tier
        >>> intervals = extract_speaker_intervals(tier)
        >>> len(intervals)  # Number of speech segments
        42
        >>> intervals[0].text
        'hello'
    """
    intervals = []

    for interval in tier:
        # Skip empty intervals (silence)
        text = interval.mark.strip() if interval.mark else ""
        if text:  # Non-empty text indicates speech
            intervals.append(
                Interval(
                    start=float(interval.minTime),
                    end=float(interval.maxTime),
                    text=text,
                )
            )

    return intervals


def find_overlaps(
    speaker_intervals: Dict[str, List[Interval]], t_start: float, t_end: float
) -> List[Overlap]:
    """Find time ranges where two or more speakers overlap.

    Args:
        speaker_intervals: Dictionary mapping speaker IDs to their intervals
        t_start: Start time for analysis window
        t_end: End time for analysis window

    Returns:
        List of Overlap objects with speaker information

    Example:
        >>> speaker_intervals = {
        ...     "SPK001": [Interval(0.0, 2.0, "a"), Interval(5.0, 7.0, "b")],
        ...     "SPK002": [Interval(1.0, 3.0, "x"), Interval(4.5, 6.0, "y")]
        ... }
        >>> overlaps = find_overlaps(speaker_intervals, 0.0, 10.0)
        >>> len(overlaps)
        2
        >>> overlaps[0].speakers
        ['SPK001', 'SPK002']
    """
    if len(speaker_intervals) < 2:
        return []

    # Collect all intervals with speaker IDs
    all_with_speaker = []
    for speaker_id, intervals in speaker_intervals.items():
        for interval in intervals:
            # Clip to analysis window
            start = max(interval.start, t_start)
            end = min(interval.end, t_end)
            if start < end:
                all_with_speaker.append((start, interval, speaker_id))

    # Sort by start time
    all_with_speaker.sort(key=lambda x: x[0])

    # Find overlapping regions
    overlaps = []
    i = 0
    n = len(all_with_speaker)

    while i < n:
        current_time, current_interval, current_speaker = all_with_speaker[i]
        current_speakers = {current_speaker}
        overlap_start = current_time
        overlap_end = current_interval.end

        # Check forward for overlapping intervals
        j = i + 1
        while j < n:
            next_time, next_interval, next_speaker = all_with_speaker[j]

            if next_time < overlap_end:
                # Overlap detected
                current_speakers.add(next_speaker)
                overlap_end = max(overlap_end, next_interval.end)
                j += 1
            else:
                break

        # Create overlap if multiple speakers
        if len(current_speakers) >= 2:
            # Check if this overlaps with previous
            if overlaps:
                prev = overlaps[-1]
                if overlap_start <= prev.end:
                    # Merge with previous overlap
                    prev.end = max(prev.end, overlap_end)
                    # Update speakers set
                    for spk in current_speakers:
                        if spk not in prev.speakers:
                            prev.speakers.append(spk)
                    prev.speakers.sort()
                else:
                    overlaps.append(
                        Overlap(
                            start=overlap_start,
                            end=overlap_end,
                            speakers=sorted(list(current_speakers)),
                        )
                    )
            else:
                overlaps.append(
                    Overlap(
                        start=overlap_start,
                        end=overlap_end,
                        speakers=sorted(list(current_speakers)),
                    )
                )

        i = j if j > i + 1 else i + 1

    # Clip overlaps to time window
    overlaps = [
        Overlap(start=max(o.start, t_start), end=min(o.end, t_end), speakers=o.speakers)
        for o in overlaps
        if o.start < o.end
    ]

    return overlaps


def calculate_solo_time(
    speaker_id: str,
    speaker_intervals: List[Interval],
    all_intervals: Dict[str, List[Interval]],
    t_start: float,
    t_end: float,
) -> float:
    """Calculate seconds where speaker speaks alone (no overlap with others).

    Args:
        speaker_id: The speaker ID to analyze
        speaker_intervals: List of intervals for the target speaker
        all_intervals: Dictionary of all speakers' intervals
        t_start: Start time for analysis window
        t_end: End time for analysis window

    Returns:
        Total solo speaking time in seconds

    Example:
        >>> speaker_intervals = [Interval(0.0, 2.0, "a")]
        >>> all_intervals = {
        ...     "SPK001": speaker_intervals,
        ...     "SPK002": [Interval(1.0, 3.0, "x")]
        ... }
        >>> solo = calculate_solo_time("SPK001", speaker_intervals, all_intervals, 0.0, 10.0)
        >>> solo
        1.0
    """
    if not speaker_intervals:
        return 0.0

    # Get other speakers' intervals
    other_intervals = []
    for other_id, intervals in all_intervals.items():
        if other_id != speaker_id:
            for interval in intervals:
                # Clip to analysis window
                start = max(interval.start, t_start)
                end = min(interval.end, t_end)
                if start < end:
                    other_intervals.append(Interval(start, end, interval.text))

    if not other_intervals:
        # No other speakers, all time is solo
        total = 0.0
        for interval in speaker_intervals:
            start = max(interval.start, t_start)
            end = min(interval.end, t_end)
            if start < end:
                total += end - start
        return total

    # Merge other intervals
    other_merged = Interval.merge(other_intervals)

    # Find non-overlapping portions for target speaker
    solo_total = 0.0

    for speaker_interval in speaker_intervals:
        # Clip to analysis window
        start = max(speaker_interval.start, t_start)
        end = min(speaker_interval.end, t_end)
        if start >= end:
            continue

        # Find portions not overlapped by others
        current_start = start
        current_end = end

        for other in other_merged:
            if other.overlaps_with(speaker_interval):
                # Split interval around overlap
                if current_start < other.start:
                    solo_total += other.start - current_start
                if current_end > other.end:
                    current_start = other.end
                else:
                    current_start = current_end
                    break

        # Add remaining solo portion
        if current_start < current_end:
            solo_total += current_end - current_start

    return solo_total


def find_solo_segments(
    speaker_id: str,
    speaker_intervals: List[Interval],
    all_intervals: Dict[str, List[Interval]],
    min_duration: float = 5.0,
) -> List[Interval]:
    """Find solo speaking segments >= min_duration.

    Args:
        speaker_id: The speaker ID to analyze
        speaker_intervals: List of intervals for the target speaker
        all_intervals: Dictionary of all speakers' intervals
        min_duration: Minimum duration for a solo segment (default: 5.0 seconds)

    Returns:
        List of Interval objects representing solo speaking segments

    Example:
        >>> speaker_intervals = [
        ...     Interval(0.0, 3.0, "a"),
        ...     Interval(10.0, 20.0, "b")
        ... ]
        >>> all_intervals = {
        ...     "SPK001": speaker_intervals,
        ...     "SPK002": [Interval(15.0, 18.0, "x")]
        ... }
        >>> solos = find_solo_segments("SPK001", speaker_intervals, all_intervals, min_duration=2.0)
        >>> len(solos)
        2
        >>> solos[1].duration
        5.0
    """
    if not speaker_intervals:
        return []

    # Get other speakers' intervals
    other_intervals = []
    for other_id, intervals in all_intervals.items():
        if other_id != speaker_id:
            other_intervals.extend(intervals)

    if not other_intervals:
        # No other speakers, all intervals are solo
        solo_intervals = [
            Interval(interval.start, interval.end, interval.text)
            for interval in speaker_intervals
            if interval.duration >= min_duration
        ]
        return Interval.merge(solo_intervals)

    # Find solo portions
    solo_segments = []

    for speaker_interval in speaker_intervals:
        solo_start = speaker_interval.start
        solo_end = speaker_interval.end

        # Check overlap with other speakers
        overlaps = []
        for other in other_intervals:
            if speaker_interval.overlaps_with(other):
                overlap = speaker_interval.overlap(other)
                if overlap:
                    overlaps.append(overlap)

        if not overlaps:
            # Entire interval is solo
            if speaker_interval.duration >= min_duration:
                solo_segments.append(speaker_interval)
        else:
            # Split at overlap boundaries
            overlaps.sort(key=lambda x: x.start)

            # Portion before first overlap
            if overlaps[0].start > solo_start:
                before_start = solo_start
                before_end = overlaps[0].start
                if (before_end - before_start) >= min_duration:
                    solo_segments.append(
                        Interval(
                            start=before_start,
                            end=before_end,
                            text=speaker_interval.text,
                        )
                    )

            # Portions between overlaps
            for i in range(len(overlaps) - 1):
                between_start = overlaps[i].end
                between_end = overlaps[i + 1].start
                if (between_end - between_start) >= min_duration:
                    solo_segments.append(
                        Interval(
                            start=between_start,
                            end=between_end,
                            text=speaker_interval.text,
                        )
                    )

            # Portion after last overlap
            if overlaps[-1].end < solo_end:
                after_start = overlaps[-1].end
                after_end = solo_end
                if (after_end - after_start) >= min_duration:
                    solo_segments.append(
                        Interval(
                            start=after_start, end=after_end, text=speaker_interval.text
                        )
                    )

    # Merge adjacent solo segments
    return Interval.merge(solo_segments)


def label_windows(
    tier_intervals: Dict[str, List[Interval]],
    t_start: float,
    t_end: float,
    hop: float = 0.5,
) -> List[Dict]:
    """Generate ground truth labels with window-based analysis.

    Creates a sliding window analysis of speaker activity. For each window,
    determines which speakers are enrolled (speaking) and which are active.

    Args:
        tier_intervals: Dictionary mapping speaker IDs to their intervals
        t_start: Start time for analysis
        t_end: End time for analysis
        hop: Window hop size in seconds (default: 0.5)

    Returns:
        List of dictionaries with keys:
            - 't_s': window start time
            - 'is_enrolled': True if any speaker is active in window
            - 'speakers': list of speaker IDs active in window

    Example:
        >>> tier_intervals = {
        ...     "SPK001": [Interval(0.0, 2.0, "a")],
        ...     "SPK002": [Interval(1.5, 3.0, "x")]
        ... }
        >>> labels = label_windows(tier_intervals, 0.0, 5.0, hop=1.0)
        >>> labels[0]
        {'t_s': 0.0, 'is_enrolled': True, 'speakers': ['SPK001']}
        >>> labels[1]
        {'t_s': 1.0, 'is_enrolled': True, 'speakers': ['SPK001', 'SPK002']}
    """
    labels = []

    # Create time points
    t = t_start
    while t < t_end:
        window_start = t
        window_end = min(t + hop, t_end)

        # Find active speakers in this window
        active_speakers = []
        for speaker_id, intervals in tier_intervals.items():
            for interval in intervals:
                if interval.overlaps_with(Interval(window_start, window_end, "")):
                    if speaker_id not in active_speakers:
                        active_speakers.append(speaker_id)
                    break  # Only need to know if speaker is active, not how many intervals

        labels.append(
            {
                "t_s": window_start,
                "is_enrolled": len(active_speakers) > 0,
                "speakers": active_speakers,
            }
        )

        t += hop

    return labels
