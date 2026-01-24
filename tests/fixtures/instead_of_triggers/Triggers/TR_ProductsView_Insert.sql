-- INSTEAD OF INSERT trigger on view
CREATE TRIGGER [dbo].[TR_ProductsView_Insert]
ON [dbo].[ProductsView]
INSTEAD OF INSERT
AS
BEGIN
    SET NOCOUNT ON;

    INSERT INTO [dbo].[Products] ([Id], [Name], [Price], [IsActive], [CreatedAt])
    SELECT [Id], [Name], [Price], 1, GETDATE()
    FROM inserted;

    INSERT INTO [dbo].[ProductHistory] ([ProductId], [Action], [NewName], [NewPrice])
    SELECT [Id], 'INSERT', [Name], [Price]
    FROM inserted;
END;
GO
