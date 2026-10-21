-- Meetily perso (fork), lot 2: meeting tags and Notion sync state.
-- Same rules as 20990101000001: only CREATE ... IF NOT EXISTS.

-- Tags with a Notion colour name (default, gray, brown, orange, yellow, green,
-- blue, purple, pink, red) so they look the same in the app and in Notion
CREATE TABLE IF NOT EXISTS tags (
    id TEXT PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    color TEXT NOT NULL DEFAULT 'default',
    created_at TEXT NOT NULL
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_tags_name ON tags(name COLLATE NOCASE);

CREATE TABLE IF NOT EXISTS meeting_tags (
    meeting_id TEXT NOT NULL,
    tag_id TEXT NOT NULL,
    PRIMARY KEY (meeting_id, tag_id),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE,
    FOREIGN KEY (tag_id) REFERENCES tags(id) ON DELETE CASCADE
);
CREATE INDEX IF NOT EXISTS idx_meeting_tags_tag_id ON meeting_tags(tag_id);

-- One Notion page per meeting; the hash skips syncs when nothing changed
CREATE TABLE IF NOT EXISTS notion_pages (
    meeting_id TEXT PRIMARY KEY NOT NULL,
    page_id TEXT,
    page_url TEXT,
    content_hash TEXT,
    synced_at TEXT,
    last_error TEXT,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
