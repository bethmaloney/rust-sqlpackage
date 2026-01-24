-- Table with composite PK (2 columns)
CREATE TABLE [dbo].[States] (
    [CountryCode] CHAR(2) NOT NULL,
    [StateCode] CHAR(3) NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    CONSTRAINT [PK_States] PRIMARY KEY ([CountryCode], [StateCode]),
    CONSTRAINT [FK_States_Countries] FOREIGN KEY ([CountryCode])
        REFERENCES [dbo].[Countries]([CountryCode])
);
GO
