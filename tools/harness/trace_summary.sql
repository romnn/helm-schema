-- Inclusive span work overlaps across workers; it is not additive wall time.
SELECT slice.name AS name, count(*) AS spans, round(sum(dur) / 1e9, 6) AS inclusive_seconds,
       round(max(dur) / 1e9, 6) AS longest_seconds
FROM slice JOIN track ON slice.track_id = track.id
WHERE dur >= 0 AND track.type = 'thread_execution'
GROUP BY slice.name
ORDER BY sum(dur) DESC;
