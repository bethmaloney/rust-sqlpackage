-- Table with full-text index
CREATE TABLE [dbo].[Documents] (
    [Id] INT NOT NULL CONSTRAINT [PK_Documents] PRIMARY KEY,
    [Title] NVARCHAR(200) NOT NULL,
    [Content] NVARCHAR(MAX) NOT NULL,
    [Author] NVARCHAR(100) NULL,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE()
);
GO

-- Create full-text catalog
CREATE FULLTEXT CATALOG [DocumentCatalog] AS DEFAULT;
GO

-- Create full-text index on Documents table
CREATE FULLTEXT INDEX ON [dbo].[Documents] (
    [Title] LANGUAGE 1033,
    [Content] LANGUAGE 1033,
    [Author] LANGUAGE 1033
)
KEY INDEX [PK_Documents] ON [DocumentCatalog]
WITH CHANGE_TRACKING AUTO;
GO
