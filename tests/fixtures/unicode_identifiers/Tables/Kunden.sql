-- German table name: Customers
CREATE TABLE [dbo].[Kunden] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Vorname] NVARCHAR(100) NOT NULL,        -- First name
    [Nachname] NVARCHAR(100) NOT NULL,       -- Last name
    [Straße] NVARCHAR(200) NULL,             -- Street (with ß)
    [Größe] DECIMAL(5,2) NULL,               -- Size (with ö)
    [Geburtsdatum] DATE NULL                 -- Birth date
);
GO
