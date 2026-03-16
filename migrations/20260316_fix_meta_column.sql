-- migrations/20260316_fix_meta_column.sql
-- Resolves: init created `metadata JSONB`, later migration added duplicate `meta JSONB`.
-- Fix: migrate data, enforce NOT NULL, drop redundant column.

-- Step A: backfill new column from old for all existing rows
UPDATE events SET meta = metadata WHERE meta IS NULL;

-- Step B: promote meta to NOT NULL with default (matches original metadata semantics)
ALTER TABLE events ALTER COLUMN meta SET NOT NULL;
ALTER TABLE events ALTER COLUMN meta SET DEFAULT '{}';

-- Step C: drop the now-redundant original column
ALTER TABLE events DROP COLUMN metadata;
