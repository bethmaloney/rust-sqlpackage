-- German table name: Products (with various Unicode chars)
CREATE TABLE [dbo].[Produkte] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Größenbezeichnung] NVARCHAR(50) NOT NULL,   -- Size designation (ö)
    [Prüfstatus] NVARCHAR(20) NULL,              -- Test status (ü)
    [Ähnlichkeit] DECIMAL(5,2) NULL,             -- Similarity (Ä)
    [日期] DATE NULL,                              -- Date (Chinese)
    [Цена] DECIMAL(18,2) NULL                    -- Price (Cyrillic)
);
GO
