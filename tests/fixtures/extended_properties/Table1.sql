-- Table with extended properties (column descriptions)
CREATE TABLE [dbo].[DocumentedTable] (
    [Id] INT NOT NULL PRIMARY KEY,
    [Name] NVARCHAR(100) NOT NULL,
    [Description] NVARCHAR(500) NULL,
    [CreatedAt] DATETIME2 NOT NULL DEFAULT GETDATE()
);
GO

-- Add extended property for table description
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'This table stores documented items with full metadata',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'DocumentedTable';
GO

-- Add extended property for column descriptions
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Unique identifier for the documented item',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'DocumentedTable',
    @level2type = N'COLUMN', @level2name = N'Id';
GO

EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Display name of the item',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'DocumentedTable',
    @level2type = N'COLUMN', @level2name = N'Name';
GO
