-- Table with composite FK referencing composite PK (2 columns)
CREATE TABLE [dbo].[Cities] (
    [Id] INT NOT NULL,
    [CountryCode] CHAR(2) NOT NULL,
    [StateCode] CHAR(3) NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    [Population] INT NULL,
    CONSTRAINT [PK_Cities] PRIMARY KEY ([Id]),
    CONSTRAINT [FK_Cities_States] FOREIGN KEY ([CountryCode], [StateCode])
        REFERENCES [dbo].[States]([CountryCode], [StateCode])
);
GO
