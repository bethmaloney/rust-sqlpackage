-- Table with named inline default constraints (Capital.Database style)
CREATE TABLE [dbo].[Entity] (
    [Id] UNIQUEIDENTIFIER NOT NULL,
    [Version] INT CONSTRAINT [DF_Entity_Version] NOT NULL DEFAULT ((0)),
    [CreatedOn] DATETIME CONSTRAINT [DF_Entity_CreatedOn] NOT NULL DEFAULT GETDATE(),
    [ModifiedOn] DATETIME CONSTRAINT [DF_Entity_ModifiedOn] NOT NULL DEFAULT GETDATE(),
    [CreatedById] UNIQUEIDENTIFIER CONSTRAINT [DF_Entity_CreatedById] NOT NULL DEFAULT ('7474C57E-BC94-4DCC-8740-9F91008ECBA0'),
    [ModifiedById] UNIQUEIDENTIFIER CONSTRAINT [DF_Entity_ModifiedById] NOT NULL DEFAULT ('7474C57E-BC94-4DCC-8740-9F91008ECBA0'),
    [IsActive] BIT CONSTRAINT [DF_Entity_IsActive] NOT NULL DEFAULT 1,
    [SortOrder] INT CONSTRAINT [DF_Entity_SortOrder] NOT NULL DEFAULT 0,
    CONSTRAINT [PK_Entity] PRIMARY KEY CLUSTERED ([Id] ASC)
);
GO

-- Table with mixed named and unnamed defaults
CREATE TABLE [dbo].[Product] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [Price] DECIMAL(18, 2) CONSTRAINT [DF_Product_Price] NOT NULL DEFAULT 0.00,
    [Quantity] INT NOT NULL DEFAULT 0,  -- unnamed default
    [IsAvailable] BIT CONSTRAINT [DF_Product_IsAvailable] NOT NULL DEFAULT 1,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE()  -- unnamed default
);
GO

-- Table with function call defaults
CREATE TABLE [dbo].[AuditLog] (
    [Id] UNIQUEIDENTIFIER CONSTRAINT [DF_AuditLog_Id] NOT NULL DEFAULT NEWID(),
    [Timestamp] DATETIME2 CONSTRAINT [DF_AuditLog_Timestamp] NOT NULL DEFAULT SYSDATETIME(),
    [Action] NVARCHAR(50) NOT NULL,
    [UserId] UNIQUEIDENTIFIER CONSTRAINT [DF_AuditLog_UserId] NULL DEFAULT NULL
);
GO
