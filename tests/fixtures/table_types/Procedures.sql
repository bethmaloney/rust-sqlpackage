-- Procedure using table type parameter
CREATE PROCEDURE [dbo].[ProcessOrderItems]
    @Items [dbo].[OrderItemsType] READONLY
AS
BEGIN
    SELECT
        ProductId,
        Quantity,
        UnitPrice,
        Quantity * UnitPrice * (1 - Discount / 100) AS LineTotal
    FROM @Items;
END
GO

-- Procedure using IdListType
CREATE PROCEDURE [dbo].[GetItemsByIds]
    @Ids [dbo].[IdListType] READONLY
AS
BEGIN
    SELECT Id FROM @Ids ORDER BY SortOrder;
END
GO
