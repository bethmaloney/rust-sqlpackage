CREATE VIEW [dbo].[ActiveProducts]
AS
SELECT
    p.[Id],
    p.[SKU],
    p.[Name],
    p.[Price],
    p.[Quantity],
    c.[Name] AS CategoryName
FROM [dbo].[Products] p
INNER JOIN [dbo].[Categories] c ON p.[CategoryId] = c.[Id];
GO
