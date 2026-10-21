-- Meetily perso (fork), lot 2b: tag suggestions proposed by the LLM for each meeting.
-- Same rules as the other fork migrations: only CREATE ... IF NOT EXISTS.

-- JSON array of suggested tag names still waiting for the user (accepted or
-- dismissed names are removed). A row with [] means "already generated, nothing left".
CREATE TABLE IF NOT EXISTS tag_suggestions (
    meeting_id TEXT PRIMARY KEY NOT NULL,
    names TEXT NOT NULL DEFAULT '[]',
    generated_at TEXT NOT NULL,
    FOREIGN KEY (meeting_id) REFERENCES meetings(id) ON DELETE CASCADE
);
