-- Meetily perso (fork): screenshots in summaries.
-- Same rules as the other fork migrations: only CREATE ... IF NOT EXISTS.

-- Notion file uploads of meeting screenshots: an attached upload id stays valid and is
-- reused on every later sync instead of sending the image again
CREATE TABLE IF NOT EXISTS notion_uploads (
    meeting_id TEXT NOT NULL,
    file TEXT NOT NULL,
    upload_id TEXT NOT NULL,
    created_at TEXT NOT NULL,
    PRIMARY KEY (meeting_id, file),
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
