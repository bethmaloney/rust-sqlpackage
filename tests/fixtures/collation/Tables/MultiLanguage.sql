-- Table with various collation specifications
CREATE TABLE [dbo].[MultiLanguage] (
    [Id] INT NOT NULL,

    -- Latin case-insensitive (default for many installations)
    [EnglishText] NVARCHAR(200) COLLATE SQL_Latin1_General_CP1_CI_AS NOT NULL,

    -- Case-sensitive comparison
    [CaseSensitiveCode] VARCHAR(50) COLLATE SQL_Latin1_General_CP1_CS_AS NOT NULL,

    -- Binary collation (fastest comparison)
    [BinaryData] VARCHAR(100) COLLATE Latin1_General_BIN NOT NULL,

    -- Japanese collation
    [JapaneseText] NVARCHAR(200) COLLATE Japanese_CI_AS NULL,

    -- Turkish (special I handling)
    [TurkishText] NVARCHAR(200) COLLATE Turkish_CI_AS NULL,

    CONSTRAINT [PK_MultiLanguage] PRIMARY KEY ([Id])
);
GO
