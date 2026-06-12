# pprof Interpretation

For CPU profiles, high flat time points to direct execution cost. High cumulative time with lower flat time points to broad caller paths or orchestration overhead.

For heap profiles, distinguish allocated bytes from in-use bytes if the profile exposes both. A large allocator function is not necessarily the ownership root.

For tree output, inspect parent chains before naming a root cause. Prefer a caller that explains the symptom and is supported by logs, metadata, or tool findings.

Do not cite this reference as final evidence. Use current task artifacts and pprof tool findings.
