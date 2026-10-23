-- Provision with a DBA identity, never with the collection worker.
-- This namespace is separate from the legacy rustdx.bars flat table.
CREATE DATABASE IF NOT EXISTS rustdx_versioned;
CREATE TABLE IF NOT EXISTS rustdx_versioned.bars (
    dataset_id String,
    release_id String,
    contract_version String,
    date Date,
    market String,
    code String,
    instrument_class String,
    open Float64,
    high Float64,
    low Float64,
    close Float64,
    amount_cny Float64,
    volume_shares Float64,
    CONSTRAINT contract CHECK contract_version = 'a-share/day/raw/cny/shares/v1',
    CONSTRAINT identity CHECK instrument_class = 'a_share' AND market IN ('sh','sz'),
    CONSTRAINT share_count CHECK volume_shares >= 0 AND volume_shares = floor(volume_shares)
) ENGINE = MergeTree
ORDER BY (dataset_id,release_id,contract_version,date,market,code,instrument_class);

-- Grant SELECT and INSERT on this table to the publisher.
-- Consumers use a separate SELECT-only role. No ALTER/DELETE/TRUNCATE/CREATE grants.
-- Do not configure TTL, ReplacingMergeTree, async inserts or destructive maintenance.
