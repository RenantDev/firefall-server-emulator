-- Firefall Server Emulator - Initial Schema

CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

CREATE TABLE accounts (
    id UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    email VARCHAR(255) UNIQUE NOT NULL,
    password_hash TEXT NOT NULL,
    display_name VARCHAR(100) NOT NULL DEFAULT '',
    is_vip BOOLEAN NOT NULL DEFAULT false,
    character_limit INTEGER NOT NULL DEFAULT 2,
    language VARCHAR(10) NOT NULL DEFAULT 'en',
    steam_id VARCHAR(64),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE characters (
    character_guid UUID PRIMARY KEY DEFAULT uuid_generate_v4(),
    account_id UUID NOT NULL REFERENCES accounts(id),
    name VARCHAR(100) NOT NULL,
    unique_name VARCHAR(100) NOT NULL,
    is_active BOOLEAN NOT NULL DEFAULT true,
    is_dev BOOLEAN NOT NULL DEFAULT false,
    gender INTEGER NOT NULL DEFAULT 0,
    race VARCHAR(20) NOT NULL DEFAULT 'human',
    title_id INTEGER NOT NULL DEFAULT 0,
    time_played_secs BIGINT NOT NULL DEFAULT 0,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    deleted_at TIMESTAMPTZ,
    -- Appearance
    head INTEGER NOT NULL DEFAULT 0,
    eye_color INTEGER NOT NULL DEFAULT 0,
    lip_color INTEGER NOT NULL DEFAULT 0,
    hair_color INTEGER NOT NULL DEFAULT 0,
    facial_hair_color INTEGER NOT NULL DEFAULT 0,
    skin_color INTEGER NOT NULL DEFAULT 0,
    voice_set INTEGER NOT NULL DEFAULT 0,
    -- Progression
    level INTEGER NOT NULL DEFAULT 1,
    current_battleframe VARCHAR(50) NOT NULL DEFAULT 'assault',
    frame_sdb_id INTEGER NOT NULL DEFAULT 76331
);

CREATE UNIQUE INDEX idx_characters_unique_name ON characters (unique_name) WHERE deleted_at IS NULL;
CREATE INDEX idx_characters_account ON characters (account_id);

-- Default test account (email: test@firefall.local, password: firefall)
-- SHA256 of "firefall" = 126cec69eb891516a7e6cf35d5180b3d104731513f8fd14f48eaae33e4360b61
INSERT INTO accounts (id, email, password_hash, display_name, is_vip, character_limit)
VALUES (
    uuid_generate_v4(),
    'test@firefall.local',
    '126cec69eb891516a7e6cf35d5180b3d104731513f8fd14f48eaae33e4360b61',
    'Tester',
    true,
    4
);
