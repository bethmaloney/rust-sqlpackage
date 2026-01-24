-- INSTEAD OF UPDATE trigger on view
CREATE TRIGGER [dbo].[TR_ProductsView_Update]
ON [dbo].[ProductsView]
INSTEAD OF UPDATE
AS
BEGIN
    SET NOCOUNT ON;

    -- Log the change
    INSERT INTO [dbo].[ProductHistory] ([ProductId], [Action], [OldName], [NewName], [OldPrice], [NewPrice])
    SELECT i.[Id], 'UPDATE', d.[Name], i.[Name], d.[Price], i.[Price]
    FROM inserted i
    INNER JOIN deleted d ON i.[Id] = d.[Id];

    -- Apply the update
    UPDATE p
    SET
        p.[Name] = i.[Name],
        p.[Price] = i.[Price],
        p.[ModifiedAt] = GETDATE()
    FROM [dbo].[Products] p
    INNER JOIN inserted i ON p.[Id] = i.[Id];
END;
GO
