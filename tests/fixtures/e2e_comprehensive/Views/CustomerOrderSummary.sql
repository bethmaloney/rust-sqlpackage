CREATE VIEW [Sales].[CustomerOrderSummary]
AS
SELECT
    c.[Id] AS CustomerId,
    c.[FirstName],
    c.[LastName],
    c.[Email],
    COUNT(o.[Id]) AS TotalOrders,
    ISNULL(SUM(o.[TotalAmount]), 0) AS TotalSpent
FROM [Sales].[Customers] c
LEFT JOIN [Sales].[Orders] o ON c.[Id] = o.[CustomerId]
GROUP BY c.[Id], c.[FirstName], c.[LastName], c.[Email];
GO
