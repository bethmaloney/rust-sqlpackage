-- Table demonstrating case-sensitive and accent-sensitive collations
CREATE TABLE [dbo].[CaseSensitive] (
    [Id] INT NOT NULL,

    -- Case-sensitive, Accent-sensitive
    [CS_AS_Column] NVARCHAR(100) COLLATE Latin1_General_CS_AS NOT NULL,

    -- Case-insensitive, Accent-sensitive
    [CI_AS_Column] NVARCHAR(100) COLLATE Latin1_General_CI_AS NOT NULL,

    -- Case-sensitive, Accent-insensitive
    [CS_AI_Column] NVARCHAR(100) COLLATE Latin1_General_CS_AI NOT NULL,

    -- Case-insensitive, Accent-insensitive
    [CI_AI_Column] NVARCHAR(100) COLLATE Latin1_General_CI_AI NOT NULL,

    CONSTRAINT [PK_CaseSensitive] PRIMARY KEY ([Id])
);
GO
