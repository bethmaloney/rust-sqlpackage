-- Extended properties for table and column descriptions
-- Tests SqlExtendedProperty element generation

-- Table description for Categories
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Product categories for organizing the catalog',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'Categories';
GO

-- Column descriptions for Categories
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Unique identifier for the category',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'Categories',
    @level2type = N'COLUMN', @level2name = N'Id';
GO

EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Display name of the category',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'Categories',
    @level2type = N'COLUMN', @level2name = N'Name';
GO

-- Table description for Products
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Product catalog with inventory and pricing information',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'Products';
GO

-- Column descriptions for Products
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Stock Keeping Unit - unique product identifier',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'Products',
    @level2type = N'COLUMN', @level2name = N'SKU';
GO

EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Unit price in the default currency',
    @level0type = N'SCHEMA', @level0name = N'dbo',
    @level1type = N'TABLE',  @level1name = N'Products',
    @level2type = N'COLUMN', @level2name = N'Price';
GO

-- Table description for Customers
EXEC sp_addextendedproperty
    @name = N'MS_Description',
    @value = N'Customer master data for the sales system',
    @level0type = N'SCHEMA', @level0name = N'Sales',
    @level1type = N'TABLE',  @level1name = N'Customers';
GO
