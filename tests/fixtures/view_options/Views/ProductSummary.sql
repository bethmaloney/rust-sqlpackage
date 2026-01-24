-- View with multiple options: SCHEMABINDING and VIEW_METADATA
CREATE VIEW [dbo].[ProductSummary]
WITH SCHEMABINDING, VIEW_METADATA
AS
SELECT
    c.[Id] AS CategoryId,
    c.[Name] AS CategoryName,
    COUNT_BIG(*) AS ProductCount,
    SUM(p.[Price]) AS TotalPrice
FROM [dbo].[Products] p
INNER JOIN [dbo].[Categories] c ON p.[CategoryId] = c.[Id]
WHERE p.[IsActive] = 1
GROUP BY c.[Id], c.[Name];
GO
