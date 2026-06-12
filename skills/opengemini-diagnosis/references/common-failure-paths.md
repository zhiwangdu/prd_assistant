# openGemini Common Diagnostic Paths

For write failures, inspect WAL/disk messages, shard ownership, PT status, and DataNode availability.

For query failures, inspect query text, time predicate presence, shard group selection, index group state, and SqlNode/DataNode connection mismatches.

For cluster membership changes, inspect MetaNode state, DataNode status transitions, `ConnID` vs `AliveConnID`, and PT owner movement.

For data missing or inconsistent results, compare RP, ShardGroup, Shard time ranges, IndexGroup, Index ownership, and measurement schema.

For high CPU or memory symptoms, combine pprof findings with log evidence and topology facts before assigning root cause.
