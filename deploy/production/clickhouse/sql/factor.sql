-- ReplacingMergeTree(version) 的最新可见行；重跑同一天时使用FINAL消除重复。
SELECT code, t.1 AS date, t.2 AS close, t.3 AS factor
FROM (
    SELECT code, argMax(tuple(date, close, factor), date) AS t
    FROM rustdx.factor FINAL
    GROUP BY code
);
