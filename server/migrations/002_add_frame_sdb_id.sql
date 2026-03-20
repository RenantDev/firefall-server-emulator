-- Migration 002: Add frame_sdb_id column to characters table
-- Run this manually if the database was created with 001_initial.sql before this column existed:
--   psql -U firefall -d firefall -f migrations/002_add_frame_sdb_id.sql
-- Or drop and recreate the database to use the updated 001_initial.sql.

DO $$
BEGIN
    IF NOT EXISTS (
        SELECT 1 FROM information_schema.columns
        WHERE table_name = 'characters' AND column_name = 'frame_sdb_id'
    ) THEN
        ALTER TABLE characters ADD COLUMN frame_sdb_id INTEGER NOT NULL DEFAULT 76331;
    END IF;
END $$;
