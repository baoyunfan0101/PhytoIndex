ATTACH DATABASE 'vividarium_base.db' AS base;
PRAGMA foreign_keys = ON;
BEGIN IMMEDIATE;

-- ============================================================
-- Source database schema
--
-- CREATE TABLE taxa (
--     id INTEGER PRIMARY KEY
--     ,parent INTEGER
--     ,category INTEGER
--     ,rank INTEGER
--     ,scientific_name TEXT
--     ,authority_year TEXT
--     ,geological_range TEXT
--     ,english_name TEXT
-- );
--
-- CREATE TABLE synonyms (
--     parent INTEGER
--     ,category INTEGER
--     ,synonym TEXT
--     ,authority_year TEXT
--     ,PRIMARY KEY (parent, synonym, authority_year)
-- );
--
-- CREATE TABLE chinese (
--     id INTEGER
--     ,is_accepted INTEGER
--     ,chinese_name TEXT
--     ,source TEXT
--     ,PRIMARY KEY (id, chinese_name)
-- );
-- ============================================================

-- Create taxonomy base db
CREATE TABLE base.taxa (
    taxon_id INTEGER PRIMARY KEY AUTOINCREMENT
    ,parent_taxon_id INTEGER
    ,rank INTEGER NOT NULL
    ,geological_range TEXT

    ,CHECK (rank IN (1, 2, 3, 4, 5))
    -- rank: 1 = kingdom; 2 = order; 3 = family; 4 = genus; 5 = species

    ,FOREIGN KEY (parent_taxon_id)
        REFERENCES taxa(taxon_id)
        ON DELETE RESTRICT
);

CREATE TABLE base.taxon_names (
    name_id INTEGER PRIMARY KEY AUTOINCREMENT
    ,taxon_id INTEGER NOT NULL
    ,name_type INTEGER NOT NULL
    ,name TEXT NOT NULL
    ,normalized_name TEXT
        GENERATED ALWAYS AS (lower(name)) STORED
    ,authority_year TEXT
    ,source TEXT

    ,UNIQUE (taxon_id, name_type, name)

    ,CHECK (name_type BETWEEN 1 AND 6)
    -- name_type: 1 = sci_name; 2 = synonym; 3 = zh_name; 4 = zh_alias; 5 = en_name; 6 = en_alias

    ,CHECK (length(trim(name)) > 0)

    ,FOREIGN KEY (taxon_id)
        REFERENCES taxa(taxon_id)
        ON DELETE CASCADE
);

-- ============================================================

-- Trim ranks
WITH RECURSIVE

-- Retained taxa
kept AS (
    SELECT
        id AS taxon_id
        ,parent AS source_parent_id
        ,CASE rank
            WHEN 60 THEN 1
            WHEN 301 THEN 2
            WHEN 401 THEN 3
            WHEN 501 THEN 4
            WHEN 601 THEN 5
        END AS target_rank
        ,NULLIF(trim(geological_range), '') AS geological_range
    FROM main.taxa
    WHERE rank IN (
        60 -- kingdom
        ,301 -- order
        ,401 -- family
        ,501 -- genus
        ,601 -- species
    )
),

-- Find all ancestors for each retained taxon until encountering a retained one.
parent_walk (
    taxon_id -- who is searching for a parent
    ,candidate_id -- parent candidate
) AS (
    -- Anchor query: Start from every retained taxon.
    SELECT
        taxon_id -- itself
        ,source_parent_id -- its parent
    FROM kept

    UNION ALL

    -- Recursive query: Continue upward while encountering a discarded taxon.
    SELECT
        walk.taxon_id -- original retained taxon
        ,parent.parent -- this candidate's parent

    -- Previous results.
    FROM parent_walk AS walk

    -- Previous results' parents.
    JOIN main.taxa AS parent
        ON walk.candidate_id = parent.id

    -- If this candidate is a discarded taxon, continue.
    WHERE parent.rank NOT IN (
        60
        ,301
        ,401
        ,501
        ,601
    )
),

-- Select the retained ancestor.
nearest_parent AS (
    SELECT
        taxon_id
        ,candidate_id AS parent_taxon_id
    FROM (
        SELECT
            walk.taxon_id
            ,walk.candidate_id
        FROM parent_walk AS walk
        JOIN main.taxa AS candidate
            ON walk.candidate_id = candidate.id
        WHERE candidate.rank IN (
            60
            ,301
            ,401
            ,501
            ,601
        )
    )
)

INSERT INTO base.taxa (
    taxon_id
    ,parent_taxon_id
    ,rank
    ,geological_range
)
SELECT
    kept.taxon_id
    ,CASE
        WHEN kept.target_rank = 1 THEN NULL
        ELSE nearest_parent.parent_taxon_id
    END -- kingdom -> no parent
    ,kept.target_rank
    ,kept.geological_range
FROM kept
LEFT JOIN nearest_parent
    ON kept.taxon_id = nearest_parent.taxon_id
ORDER BY
    kept.target_rank
    ,kept.taxon_id;

-- ============================================================

-- Import scientific names
INSERT INTO base.taxon_names (
    taxon_id
    ,name_type
    ,name
    ,authority_year
    ,source
)
SELECT
    source.id
    ,1
    ,source.scientific_name
    ,NULLIF(trim(source.authority_year), '')
    ,'biolib'
FROM main.taxa AS source
JOIN base.taxa AS retained
    ON source.id = retained.taxon_id
ORDER BY
    source.id;

-- ============================================================

-- Import synonyms
INSERT INTO base.taxon_names (
    taxon_id
    ,name_type
    ,name
    ,authority_year
    ,source
)
SELECT
    synonym.parent
    ,2
    ,synonym.synonym
    ,synonym.authority_year
    ,'biolib'
FROM (
    SELECT
        parent
        ,synonym
        ,MAX(NULLIF(trim(authority_year), '')) AS authority_year
    FROM main.synonyms
    GROUP BY
        parent
        ,synonym
) AS synonym
JOIN base.taxa AS retained
    ON synonym.parent = retained.taxon_id
ORDER BY
    synonym.parent
    ,synonym.synonym
    ,synonym.authority_year;

-- ============================================================

-- Import Chinese names
INSERT INTO base.taxon_names (
    taxon_id
    ,name_type
    ,name
    ,authority_year
    ,source
)
SELECT
    chinese.id
    ,CASE
        WHEN chinese.is_accepted = 1 THEN 3
        ELSE 4
    END
    ,chinese.chinese_name
    ,NULL
    ,NULLIF(trim(chinese.source), '')
FROM main.chinese AS chinese
JOIN base.taxa AS retained
    ON chinese.id = retained.taxon_id
ORDER BY
    chinese.id
    ,chinese.is_accepted DESC
    ,chinese.chinese_name;

-- ============================================================

-- Import English names
INSERT INTO base.taxon_names (
    taxon_id
    ,name_type
    ,name
    ,authority_year
    ,source
)
SELECT
    source.id
    ,5
    ,source.english_name
    ,NULL
    ,'biolib'
FROM main.taxa AS source
JOIN base.taxa AS retained
    ON source.id = retained.taxon_id
WHERE source.english_name IS NOT NULL
    AND trim(source.english_name) <> ''
ORDER BY
    source.id;

-- ============================================================

COMMIT;
DETACH DATABASE base;
