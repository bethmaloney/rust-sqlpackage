-- Table with single-column PK (for reference)
CREATE TABLE [dbo].[Countries] (
    [CountryCode] CHAR(2) NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    CONSTRAINT [PK_Countries] PRIMARY KEY ([CountryCode])
);
GO
