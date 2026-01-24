-- INSTEAD OF DELETE trigger on view (soft delete pattern)
CREATE TRIGGER [dbo].[TR_ProductsView_Delete]
ON [dbo].[ProductsView]
INSTEAD OF DELETE
AS
BEGIN
    SET NOCOUNT ON;

    -- Log the deletion
    INSERT INTO [dbo].[ProductHistory] ([ProductId], [Action], [OldName], [OldPrice])
    SELECT [Id], 'DELETE', [Name], [Price]
    FROM deleted;

    -- Soft delete instead of hard delete
    UPDATE p
    SET p.[IsActive] = 0, p.[ModifiedAt] = GETDATE()
    FROM [dbo].[Products] p
    INNER JOIN deleted d ON p.[Id] = d.[Id];
END;
GO
