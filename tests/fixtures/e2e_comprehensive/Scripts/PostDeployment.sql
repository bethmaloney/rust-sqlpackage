/*
 Post-Deployment Script
 This script runs after the main deployment to seed initial data
*/

-- Seed Categories
IF NOT EXISTS (SELECT 1 FROM [dbo].[Categories] WHERE [Name] = 'Electronics')
BEGIN
    SET IDENTITY_INSERT [dbo].[Categories] ON;
    INSERT INTO [dbo].[Categories] ([Id], [Name], [Description], [IsActive], [CreatedAt])
    VALUES
        (1, 'Electronics', 'Electronic devices and accessories', 1, GETDATE()),
        (2, 'Clothing', 'Apparel and fashion items', 1, GETDATE()),
        (3, 'Books', 'Physical and digital books', 1, GETDATE());
    SET IDENTITY_INSERT [dbo].[Categories] OFF;
END
GO
