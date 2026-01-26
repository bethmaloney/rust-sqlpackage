CREATE TABLE [dbo].[Categories] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [Name] NVARCHAR(100) NOT NULL,
    [Description] NVARCHAR(500) NULL,
    [IsActive] BIT NOT NULL CONSTRAINT [DF_Categories_IsActive] DEFAULT (1),
    [CreatedAt] DATETIME NOT NULL CONSTRAINT [DF_Categories_CreatedAt] DEFAULT (GETDATE()),

    CONSTRAINT [PK_Categories] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [UQ_Categories_Name] UNIQUE ([Name])
);
GO
