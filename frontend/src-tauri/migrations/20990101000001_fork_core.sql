-- Meetily perso (fork) core schema.
-- Fork rules: only CREATE ... IF NOT EXISTS, never ALTER or edit upstream tables,
-- so an upstream binary can still open this database once the fork rows are
-- removed from _sqlx_migrations (DELETE FROM _sqlx_migrations WHERE version >= 20990000000000).

-- Key/value store for every fork setting (cloud models, vision, UI language, ...)
CREATE TABLE IF NOT EXISTS app_preferences (
    key TEXT PRIMARY KEY NOT NULL,
    value TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

-- Meeting context given to the AI (before, during or after the meeting)
CREATE TABLE IF NOT EXISTS meeting_context (
    meeting_id TEXT PRIMARY KEY NOT NULL,
    context_text TEXT,
    glossary TEXT,
    meeting_type TEXT,            -- 'presentiel' | 'visio'
    extra_instructions TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

CREATE TABLE IF NOT EXISTS meeting_participants (
    id TEXT PRIMARY KEY NOT NULL,
    meeting_id TEXT NOT NULL,
    name TEXT NOT NULL,
    role TEXT,
    company TEXT,
    is_self INTEGER NOT NULL DEFAULT 0,
    sort_order INTEGER NOT NULL DEFAULT 0,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_meeting_participants_meeting_id ON meeting_participants(meeting_id);

-- Address book of people already met, for participant autocomplete
CREATE TABLE IF NOT EXISTS contacts (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    role TEXT,
    company TEXT,
    last_used_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_contacts_name_company ON contacts(name, IFNULL(company, ''));

-- Display names of diarized speakers ('speaker_0', 'mic', 'system', ...) per meeting.
-- transcripts.speaker keeps the raw key; renaming a speaker only touches this table.
CREATE TABLE IF NOT EXISTS meeting_speakers (
    meeting_id TEXT NOT NULL,
    speaker_key TEXT NOT NULL,
    display_name TEXT,
    participant_id TEXT,
    source TEXT NOT NULL DEFAULT 'auto',   -- 'auto' | 'manual' | 'mic-anchor'
    confidence REAL,
    updated_at TEXT NOT NULL,
    PRIMARY KEY (meeting_id, speaker_key),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);

-- Cloud cost tracking (live STT, diarization, vision, naming)
CREATE TABLE IF NOT EXISTS meeting_usage (
    id TEXT PRIMARY KEY NOT NULL,
    meeting_id TEXT NOT NULL,
    kind TEXT NOT NULL,
    model TEXT,
    audio_seconds REAL,
    cost_usd REAL,
    created_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_meeting_usage_meeting_id ON meeting_usage(meeting_id);
