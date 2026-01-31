CREATE TYPE [dbo].[TableType4] AS TABLE
(
    [Id] INT NOT NULL,
    [Name] NVARCHAR(100) NOT NULL,
    [Amount] DECIMAL(18, 2) NOT NULL,
    [IsActive] BIT NOT NULL
);
GO
