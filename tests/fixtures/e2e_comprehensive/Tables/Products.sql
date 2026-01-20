CREATE TABLE [dbo].[Products] (
    [Id] INT NOT NULL IDENTITY(1, 1),
    [SKU] NVARCHAR(50) NOT NULL,
    [Name] NVARCHAR(200) NOT NULL,
    [CategoryId] INT NOT NULL,
    [Price] DECIMAL(18, 2) NOT NULL,
    [Quantity] INT NOT NULL,
    [ImageData] VARBINARY(MAX) NULL,
    [IsActive] BIT NOT NULL,
    [CreatedAt] DATETIME NOT NULL,
    [ModifiedAt] DATETIME NULL,

    CONSTRAINT [PK_Products] PRIMARY KEY CLUSTERED ([Id]),
    CONSTRAINT [FK_Products_Categories] FOREIGN KEY ([CategoryId]) REFERENCES [dbo].[Categories]([Id]),
    CONSTRAINT [UQ_Products_SKU] UNIQUE ([SKU]),
    CONSTRAINT [CK_Products_Price] CHECK ([Price] >= 0),
    CONSTRAINT [CK_Products_Quantity] CHECK ([Quantity] >= 0),
    CONSTRAINT [DF_Products_IsActive] DEFAULT (1) FOR [IsActive],
    CONSTRAINT [DF_Products_CreatedAt] DEFAULT (GETDATE()) FOR [CreatedAt]
);
GO
