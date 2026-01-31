CREATE TABLE [dbo].[Table32] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [Name] NVARCHAR(100) NOT NULL,
    [Description] NVARCHAR(500) NULL,
    [Amount] DECIMAL(18, 2) NOT NULL CONSTRAINT [DF_Table32_Amount] DEFAULT (0),
    [Quantity] INT NOT NULL CONSTRAINT [DF_Table32_Quantity] DEFAULT (0),
    [IsActive] BIT NOT NULL CONSTRAINT [DF_Table32_IsActive] DEFAULT (1),
    [CreatedDate] DATETIME NOT NULL CONSTRAINT [DF_Table32_CreatedDate] DEFAULT (GETDATE()),
    [ModifiedDate] DATETIME NULL,
    [Status] NVARCHAR(50) NULL,
    [Code] NVARCHAR(20) NOT NULL,
    CONSTRAINT [PK_Table32] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [UQ_Table32_Code] UNIQUE ([Code]),
    CONSTRAINT [CK_Table32_Amount] CHECK ([Amount] >= 0),
    CONSTRAINT [CK_Table32_Quantity] CHECK ([Quantity] >= 0)
);
GO
